//! Caixa-preta: registro de tudo que entra e sai do drone.

use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    Tx,
    Rx,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Cmd,
    State,
    Video,
    App,
    Core,
}

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    /// Milissegundos desde o início da sessão.
    pub t_ms: u64,
    pub dir: Dir,
    pub ch: Channel,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hex: Option<String>,
}

type Listener = Arc<dyn Fn(&Entry) + Send + Sync>;

pub struct Blackbox {
    start: Instant,
    start_unix_ms: u64,
    capacity: usize,
    entries: Mutex<VecDeque<Entry>>,
    listener: Mutex<Option<Listener>>,
}

impl Blackbox {
    pub fn new(capacity: usize) -> Self {
        let start_unix_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        Blackbox {
            start: Instant::now(),
            start_unix_ms,
            capacity,
            entries: Mutex::new(VecDeque::with_capacity(capacity.min(4096))),
            listener: Mutex::new(None),
        }
    }

    pub fn set_listener(&self, listener: Option<Listener>) {
        *self.listener.lock().unwrap() = listener;
    }

    pub fn record(&self, dir: Dir, ch: Channel, text: impl Into<String>, bytes: Option<&[u8]>) {
        let entry =
            Entry { t_ms: self.start.elapsed().as_millis() as u64, dir, ch, text: text.into(), hex: bytes.map(to_hex) };
        let listener = self.listener.lock().unwrap().clone();
        if let Some(l) = listener {
            l(&entry);
        }
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= self.capacity {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    pub fn info(&self, ch: Channel, text: impl Into<String>) {
        self.record(Dir::Info, ch, text, None);
    }

    pub fn snapshot(&self) -> Vec<Entry> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Exporta em JSONL: uma linha de cabeçalho seguida de uma linha por entrada.
    pub fn export_jsonl(&self, mut w: impl Write) -> io::Result<()> {
        let header = serde_json::json!({
            "tello_log": 1,
            "started_unix_ms": self.start_unix_ms,
            "client_version": env!("CARGO_PKG_VERSION"),
        });
        writeln!(w, "{header}")?;
        for e in self.snapshot() {
            serde_json::to_writer(&mut w, &e)?;
            writeln!(w)?;
        }
        w.flush()
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_drops_oldest() {
        let bb = Blackbox::new(3);
        for i in 0..5 {
            bb.info(Channel::Core, format!("e{i}"));
        }
        let texts: Vec<_> = bb.snapshot().into_iter().map(|e| e.text).collect();
        assert_eq!(texts, ["e2", "e3", "e4"]);
    }

    #[test]
    fn exports_jsonl_with_header() {
        let bb = Blackbox::new(10);
        bb.record(Dir::Tx, Channel::App, "takeoff", Some(&[0xcc, 0x58]));
        let mut out = Vec::new();
        bb.export_jsonl(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"tello_log\":1"));
        assert!(lines[1].contains("\"hex\":\"cc 58\""));
        assert!(lines[1].contains("\"dir\":\"tx\""));
    }
}
