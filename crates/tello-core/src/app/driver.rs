use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::commands::{translate, AppCommand};
use super::flight_data::{parse_alt_limit, parse_light, parse_version, parse_wifi, FlightData};
use super::log_data::parse_attitude;
use super::packet::{id, kind, Packet};
use super::photo::{self, Download, Progress};
use super::stick;
use crate::blackbox::{to_hex, Channel, Dir};
use crate::client::Driver;
use crate::control::Shared;
use crate::net::{bind_udp, is_timeout, set_poll_timeout};
use crate::util::{now_utc, utc_stamp};
use crate::video::NalAssembler;
use crate::{Action, Capabilities, Config, DroneEvent, Error, Model, Protocol, Sticks, WarningCode};

/// Sticks a 50 Hz, como o app oficial.
const TICK: Duration = Duration::from_millis(20);
const ACK_TIMEOUT: Duration = Duration::from_millis(800);
const SPS_INTERVAL: Duration = Duration::from_secs(1);

const NO_STATE_HINT: &str = "O drone aceitou a conexão, mas a telemetria não chega. \
     Verifique o Firewall do Windows para redes públicas.";
const NO_VIDEO_HINT: &str = "O vídeo foi pedido, mas nada chegou na porta de vídeo. \
     Verifique o Firewall do Windows para redes públicas.";

pub(crate) struct AppDriver {
    inner: Arc<Inner>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

struct Inner {
    shared: Arc<Shared>,
    sock: UdpSocket,
    drone: SocketAddr,
    video_port: u16,
    photo_dir: Option<PathBuf>,
    seq: AtomicU16,
    fast: AtomicBool,
    video_on: AtomicBool,
    cmd_lock: Mutex<()>,
    acks: Mutex<Receiver<(u16, Vec<u8>)>>,
    ack_tx: Mutex<Sender<(u16, Vec<u8>)>>,
    handshake_tx: Mutex<Option<Sender<()>>>,
    caps: Mutex<Capabilities>,
    download: Mutex<Option<Download>>,
    video_watch: Mutex<(Option<Instant>, Option<Instant>, bool)>,
}

fn app_capabilities() -> Capabilities {
    Capabilities {
        protocol: Protocol::App,
        model: Model::Tello,
        flips: true,
        flips_diagonal: true,
        throw_and_go: true,
        palm_land: true,
        bounce: true,
        smart_video: true,
        hi_res_photo: true,
        fast_mode: true,
        max_height: true,
        video_bitrate: true,
        stop: true,
        ..Default::default()
    }
}

pub(crate) fn connect(cfg: &Config, shared: Arc<Shared>) -> Result<AppDriver, Error> {
    let sock = bind_udp(cfg.local_command_port, "comandos")?;
    set_poll_timeout(&sock)?;
    let video_sock = bind_udp(cfg.app_video_port, "vídeo")?;
    set_poll_timeout(&video_sock)?;
    let video_port = video_sock.local_addr()?.port();

    let (ack_tx, ack_rx) = mpsc::channel();
    let inner = Arc::new(Inner {
        shared,
        sock,
        drone: SocketAddr::new(cfg.drone_ip, cfg.command_port),
        video_port,
        photo_dir: cfg.photo_dir.clone(),
        seq: AtomicU16::new(0),
        fast: AtomicBool::new(false),
        video_on: AtomicBool::new(false),
        cmd_lock: Mutex::new(()),
        acks: Mutex::new(ack_rx),
        ack_tx: Mutex::new(ack_tx),
        handshake_tx: Mutex::new(None),
        caps: Mutex::new(app_capabilities()),
        download: Mutex::new(None),
        video_watch: Mutex::new((None, None, false)),
    });
    let driver = AppDriver { inner: inner.clone(), threads: Mutex::new(Vec::new()) };
    driver.spawn("tello-app-recv", {
        let inner = inner.clone();
        let sock = inner.sock.try_clone()?;
        move || inner.recv_loop(sock)
    });
    driver.spawn("tello-app-video", {
        let inner = inner.clone();
        move || inner.video_loop(video_sock)
    });
    if let Err(e) = inner.handshake() {
        driver.shutdown();
        return Err(e);
    }
    inner.shared.mark_connected();
    inner.send(Packet::new(kind::GET, id::VERSION, vec![]));
    inner.send(Packet::new(kind::GET, id::ALT_LIMIT, vec![]));
    driver.spawn("tello-app-control", {
        let inner = inner.clone();
        move || inner.control_loop()
    });
    Ok(driver)
}

impl AppDriver {
    fn spawn(&self, name: &str, f: impl FnOnce() + Send + 'static) {
        let handle = thread::Builder::new().name(name.into()).spawn(f).expect("falha ao criar thread");
        self.threads.lock().unwrap().push(handle);
    }
}

impl Driver for AppDriver {
    fn caps(&self) -> Capabilities {
        self.inner.caps.lock().unwrap().clone()
    }

    fn action(&self, action: &Action) -> Result<String, Error> {
        self.inner.action(action)
    }

    fn shutdown(&self) {
        self.inner.shared.running.store(false, Ordering::SeqCst);
        let threads: Vec<_> = self.threads.lock().unwrap().drain(..).collect();
        for t in threads {
            if t.thread().id() != thread::current().id() {
                let _ = t.join();
            }
        }
    }
}

impl Drop for AppDriver {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Inner {
    fn handshake(&self) -> Result<(), Error> {
        let (tx, rx) = mpsc::channel();
        *self.handshake_tx.lock().unwrap() = Some(tx);
        let mut req = b"conn_req:".to_vec();
        req.extend_from_slice(&self.video_port.to_le_bytes());
        for _ in 0..4 {
            self.send_bytes(&req, format!("conn_req (vídeo na porta {})", self.video_port));
            if rx.recv_timeout(Duration::from_millis(1500)).is_ok() {
                return Ok(());
            }
        }
        Err(Error::Timeout(
            "o drone não respondeu ao conn_req. Confira se o computador está no Wi-Fi TELLO-XXXXXX, \
             se o drone está ligado e se o app oficial está fechado."
                .into(),
        ))
    }

    fn next_seq(&self) -> u16 {
        self.seq.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
    }

    fn send_bytes(&self, bytes: &[u8], label: String) {
        match self.sock.send_to(bytes, self.drone) {
            Ok(_) => self.shared.blackbox.record(Dir::Tx, Channel::App, label, Some(bytes)),
            Err(e) => self.shared.blackbox.info(Channel::App, format!("falha ao enviar {label}: {e}")),
        }
    }

    /// Envia um pacote com o próximo número de sequência.
    fn send(&self, mut p: Packet) {
        p.seq = self.next_seq();
        self.send_bytes(&p.encode(), p.describe());
    }

    fn action(&self, action: &Action) -> Result<String, Error> {
        match translate(action)? {
            AppCommand::Emergency => {
                for _ in 0..3 {
                    self.send_bytes(b"emergency", "emergency".into());
                }
                self.shared.set_sticks(Sticks::NEUTRAL);
                self.shared.set_flying(false);
                Ok("ok".into())
            }
            AppCommand::Hover => {
                self.shared.set_sticks(Sticks::NEUTRAL);
                Ok("ok".into())
            }
            AppCommand::SetFast(on) => {
                self.fast.store(on, Ordering::Relaxed);
                self.shared.patch_telemetry(|t| t.fast_mode = Some(on));
                Ok("ok".into())
            }
            AppCommand::VideoOn => {
                self.video_on.store(true, Ordering::Relaxed);
                *self.video_watch.lock().unwrap() = (Some(Instant::now()), None, false);
                self.send(Packet::new(kind::DATA2, id::VIDEO_SPS_PPS, vec![]));
                Ok("ok".into())
            }
            AppCommand::VideoOff => {
                self.video_on.store(false, Ordering::Relaxed);
                self.video_watch.lock().unwrap().0 = None;
                Ok("ok".into())
            }
            AppCommand::Send(p) => {
                // O estado "voando" vem da telemetria (em_sky); a decolagem só o antecipa.
                let takeoff = p.cmd == id::TAKEOFF;
                let result = self.send_and_wait(p);
                if takeoff && result.is_ok() {
                    self.shared.set_flying(true);
                }
                result
            }
        }
    }

    /// Envia e espera a confirmação com o mesmo ID. Sem confirmação, considera enviado.
    fn send_and_wait(&self, p: Packet) -> Result<String, Error> {
        let _lock = self.cmd_lock.lock().unwrap();
        let rx = self.acks.lock().unwrap();
        while rx.try_recv().is_ok() {}
        let cmd = p.cmd;
        self.send(p);
        let deadline = Instant::now() + ACK_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining) {
                Ok((c, payload)) if c == cmd => {
                    return match payload.first() {
                        None | Some(0) => Ok("ok".into()),
                        Some(code) => Err(Error::Refused(format!("{} recusado (código 0x{code:02x})", id::name(cmd)))),
                    };
                }
                Ok(_) => continue,
                Err(RecvTimeoutError::Timeout) => {
                    self.shared.blackbox.info(Channel::App, format!("{} sem confirmação", id::name(cmd)));
                    return Ok("enviado".into());
                }
                Err(RecvTimeoutError::Disconnected) => return Err(Error::NotConnected),
            }
        }
    }

    fn recv_loop(&self, sock: UdpSocket) {
        let mut buf = vec![0u8; 4096];
        while self.shared.running() {
            match sock.recv_from(&mut buf) {
                Ok((n, from)) if from.ip() == self.drone.ip() => self.handle(&buf[..n]),
                Ok(_) => {}
                Err(e) if is_timeout(&e) => {}
                Err(e) => {
                    self.shared.blackbox.info(Channel::App, format!("erro no recv: {e}"));
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }

    fn handle(&self, data: &[u8]) {
        if data.starts_with(b"conn_ack") {
            self.shared.blackbox.record(Dir::Rx, Channel::App, "conn_ack", Some(data));
            if let Some(tx) = self.handshake_tx.lock().unwrap().take() {
                let _ = tx.send(());
            }
            return;
        }
        let p = match Packet::decode(data) {
            Ok(p) => p,
            Err(e) => {
                let text = String::from_utf8_lossy(data);
                let label = if text.chars().all(|c| !c.is_control()) {
                    format!("texto: {text}")
                } else {
                    format!("pacote inválido ({e:?})")
                };
                self.shared.blackbox.record(Dir::Rx, Channel::App, label, Some(data));
                return;
            }
        };
        // Mensagens grandes e frequentes vão sem hex para não lotar a caixa-preta.
        let hex = !matches!(p.cmd, id::LOG_DATA | id::FILE_DATA);
        self.shared.blackbox.record(Dir::Rx, Channel::App, p.describe(), hex.then_some(data));

        match p.cmd {
            id::FLIGHT => match FlightData::parse(&p.payload) {
                Some(fd) => {
                    self.shared.set_flying(fd.em_sky);
                    self.shared.update_telemetry(|t| fd.apply(t));
                }
                None => self.shared.blackbox.info(Channel::App, "flight data curto demais"),
            },
            id::WIFI => {
                if let Some(w) = parse_wifi(&p.payload) {
                    self.shared.patch_telemetry(|t| t.wifi_strength = Some(w));
                }
            }
            id::LIGHT => {
                if let Some(l) = parse_light(&p.payload) {
                    self.shared.patch_telemetry(|t| t.light_strength = Some(l));
                }
            }
            id::LOG_DATA => {
                if let Some(att) = parse_attitude(&p.payload) {
                    self.shared.patch_telemetry(|t| {
                        t.pitch_deg = Some(att.pitch_deg.round() as i32);
                        t.roll_deg = Some(att.roll_deg.round() as i32);
                        t.yaw_deg = Some(att.yaw_deg.round() as i32);
                    });
                }
            }
            id::LOG_HEADER => {
                // Sem esta confirmação o drone continua reenviando o cabeçalho.
                let log_id = p.payload.get(..2).unwrap_or(&[0, 0]);
                self.send(Packet::new(kind::DATA1, id::LOG_HEADER, vec![0, log_id[0], log_id[1]]));
            }
            id::TIME => self.send_time(),
            id::VERSION => {
                if let Some(v) = parse_version(&p.payload) {
                    self.caps.lock().unwrap().firmware = Some(v);
                }
            }
            id::ALT_LIMIT => {
                if let Some(m) = parse_alt_limit(&p.payload) {
                    self.shared.blackbox.info(Channel::App, format!("limite de altura do drone: {m} m"));
                }
            }
            id::FILE_SIZE => self.start_download(&p.payload),
            id::FILE_DATA => self.receive_fragment(&p.payload),
            _ => {
                let _ = self.ack_tx.lock().unwrap().send((p.cmd, p.payload));
            }
        }
    }

    fn send_time(&self) {
        let t = now_utc();
        let mut payload = vec![0u8];
        for v in
            [t.year as u16, t.month as u16, t.day as u16, t.hour as u16, t.minute as u16, t.second as u16, t.millis]
        {
            payload.extend_from_slice(&v.to_le_bytes());
        }
        self.send(Packet::new(kind::DATA1, id::TIME, payload));
    }

    fn start_download(&self, payload: &[u8]) {
        match photo::parse_announce(payload) {
            Some(a) if a.size > 0 => {
                self.shared.blackbox.info(Channel::App, format!("recebendo foto {} ({} KB)", a.file_id, a.size / 1024));
                *self.download.lock().unwrap() = Some(Download::new(a));
            }
            _ => self.shared.blackbox.info(Channel::App, "anúncio de arquivo sem dados"),
        }
    }

    fn receive_fragment(&self, payload: &[u8]) {
        let Some(frag) = photo::parse_fragment(payload) else { return };
        let mut slot = self.download.lock().unwrap();
        let Some(dl) = slot.as_mut() else { return };
        if let Progress::ChunkDone(chunk) = dl.add(frag) {
            self.send(photo::chunk_ack(dl.announce.file_id, chunk));
        }
        if dl.is_complete() {
            let dl = slot.take().unwrap();
            drop(slot);
            let (file_id, size, last) = (dl.announce.file_id, dl.announce.size, dl.last_chunk());
            for p in photo::completion_acks(file_id, last, size) {
                self.send(p);
            }
            self.save_photo(dl.into_bytes());
        }
    }

    fn save_photo(&self, jpeg: Vec<u8>) {
        let dir = self.photo_dir.clone().unwrap_or_else(std::env::temp_dir);
        let path = dir.join(format!("tello-foto-{}.jpg", utc_stamp()));
        let result = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &jpeg));
        match result {
            Ok(()) => {
                let path = path.display().to_string();
                self.shared.blackbox.info(Channel::App, format!("foto salva em {path}"));
                self.shared.emit(DroneEvent::PhotoSaved { path });
            }
            Err(e) => self.shared.warn(WarningCode::Other, format!("Não foi possível salvar a foto: {e}")),
        }
    }

    fn video_loop(&self, sock: UdpSocket) {
        let mut buf = vec![0u8; 4096];
        let mut asm = NalAssembler::new();
        let (mut packets, mut bytes, mut frames, mut last_hdr) = (0usize, 0usize, 0usize, [0u8; 2]);
        let mut since = Instant::now();
        while self.shared.running() {
            match sock.recv_from(&mut buf) {
                Ok((n, _)) if n > 2 => {
                    self.video_watch.lock().unwrap().1 = Some(Instant::now());
                    if !self.video_on.load(Ordering::Relaxed) {
                        continue;
                    }
                    // Os 2 primeiros bytes são um cabeçalho próprio (sequência do frame/fragmento).
                    last_hdr = [buf[0], buf[1]];
                    packets += 1;
                    bytes += n;
                    for f in asm.push(&buf[2..n]) {
                        frames += 1;
                        self.shared.emit(DroneEvent::Video(f));
                    }
                }
                Ok(_) => {}
                Err(e) if is_timeout(&e) => {}
                Err(_) => thread::sleep(Duration::from_millis(100)),
            }
            if since.elapsed() >= Duration::from_secs(1) {
                if packets > 0 {
                    self.shared.blackbox.record(
                        Dir::Rx,
                        Channel::Video,
                        format!(
                            "{packets} pacotes, {} KB, {frames} frames, último cabeçalho {}",
                            bytes / 1024,
                            to_hex(&last_hdr)
                        ),
                        None,
                    );
                }
                (packets, bytes, frames) = (0, 0, 0);
                since = Instant::now();
            }
        }
    }

    fn control_loop(self: &Arc<Self>) {
        let mut last_logged = String::new();
        let mut last_log = Instant::now();
        let mut last_sps = Instant::now();
        // A primeira checagem roda já no início, para aplicar o limite de altura.
        let mut last_check = Instant::now().checked_sub(Duration::from_secs(2)).unwrap_or_else(Instant::now);
        let mut sent_alt_limit: Option<u16> = None;
        let mut next = Instant::now();
        while self.shared.running() {
            if self.shared.take_auto_land() {
                let inner = self.clone();
                thread::spawn(move || {
                    let _ = inner.action(&Action::Land);
                });
            }
            let sticks = self.shared.effective_sticks();
            let fast = self.fast.load(Ordering::Relaxed);
            let t = now_utc();
            let payload = stick::payload(sticks, fast, (t.hour, t.minute, t.second, t.millis));
            // Sticks vão com sequência 0, como no app oficial.
            let bytes = Packet::new(kind::DATA2, id::STICK, payload.to_vec()).encode();
            if let Err(e) = self.sock.send_to(&bytes, self.drone) {
                self.shared.blackbox.info(Channel::App, format!("falha nos sticks: {e}"));
            }
            let summary = format!(
                "stick r{:.2} p{:.2} t{:.2} y{:.2}{}",
                sticks.roll,
                sticks.pitch,
                sticks.throttle,
                sticks.yaw,
                if fast { " rápido" } else { "" }
            );
            if summary != last_logged || last_log.elapsed() >= Duration::from_secs(1) {
                self.shared.blackbox.record(Dir::Tx, Channel::App, summary.clone(), Some(&bytes));
                last_logged = summary;
                last_log = Instant::now();
            }
            if self.video_on.load(Ordering::Relaxed) && last_sps.elapsed() >= SPS_INTERVAL {
                // Pede SPS/PPS/keyframe periodicamente para o vídeo (re)começar rápido.
                self.send(Packet::new(kind::DATA2, id::VIDEO_SPS_PPS, vec![]));
                last_sps = Instant::now();
            }
            if last_check.elapsed() >= Duration::from_secs(1) {
                last_check = Instant::now();
                self.shared.check_link(NO_STATE_HINT);
                self.check_video();
                let wanted = self.shared.safety().max_height_cm.map(|cm| (cm as u32).div_ceil(100).clamp(1, 30) as u16);
                if let Some(m) = wanted.filter(|m| Some(*m) != sent_alt_limit) {
                    self.send(Packet::new(kind::SET, id::SET_ALT_LIMIT, m.to_le_bytes().to_vec()));
                    sent_alt_limit = Some(m);
                }
            }
            next += TICK;
            let now = Instant::now();
            if next > now {
                thread::sleep(next - now);
            } else {
                next = now;
            }
        }
    }

    fn check_video(&self) {
        let mut w = self.video_watch.lock().unwrap();
        let (Some(on_at), None, false) = *w else { return };
        if on_at.elapsed() > Duration::from_secs(4) {
            w.2 = true;
            drop(w);
            self.shared.warn(WarningCode::NoVideo, NO_VIDEO_HINT);
        }
    }
}
