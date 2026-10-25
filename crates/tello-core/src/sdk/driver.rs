use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::commands::{self, capabilities, classify, parse_sdk_version, Kind, SdkCommand};
use super::state::parse_state;
use crate::blackbox::{Channel, Dir};
use crate::client::Driver;
use crate::control::Shared;
use crate::net::{bind_udp, is_timeout, set_poll_timeout};
use crate::video::NalAssembler;
use crate::{Action, Capabilities, Config, DroneEvent, Error, VideoPolicy, VideoStats, WarningCode};

/// Intervalo do loop de controle (20 Hz).
const TICK: Duration = Duration::from_millis(50);
/// Tamanho dos fragmentos de vídeo; um pacote menor marca o fim do frame.
const VIDEO_CHUNK: usize = 1460;
const NO_VIDEO_AFTER: Duration = Duration::from_secs(4);
/// O estado do SDK não traz o sinal Wi-Fi: ele é consultado com `wifi?`.
const WIFI_POLL: Duration = Duration::from_secs(5);
/// Espera curta: a consulta nunca deve atrasar um comando do usuário.
const POLL_TIMEOUT: Duration = Duration::from_millis(500);

const NO_STATE_HINT: &str = "O drone responde aos comandos, mas a telemetria (UDP 8890) não chega. \
     Normalmente é o Firewall do Windows bloqueando a rede do Tello: permita o app em redes \
     públicas ou rode o instalador novamente.";
const NO_VIDEO_HINT: &str = "O vídeo foi ligado, mas nenhum pacote chegou na porta UDP 11111. \
     Verifique o Firewall do Windows para redes públicas.";

pub(crate) struct SdkDriver {
    inner: Arc<Inner>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

struct Inner {
    shared: Arc<Shared>,
    sock: UdpSocket,
    drone: SocketAddr,
    responses: Mutex<Receiver<String>>,
    cmd_lock: Mutex<()>,
    /// Uma manobra está em andamento: não enviar `rc`.
    paused: AtomicBool,
    caps: Mutex<Capabilities>,
    video: Mutex<VideoWatch>,
    /// Nível de taxa fixado pelo app (`setbitrate`, SDK 3).
    video_level: Mutex<Option<u8>>,
}

#[derive(Default)]
struct VideoWatch {
    stream_on_at: Option<Instant>,
    last_packet: Option<Instant>,
    warned: bool,
}

pub(crate) fn connect(cfg: &Config, shared: Arc<Shared>) -> Result<SdkDriver, Error> {
    let sock = bind_udp(cfg.local_command_port, "comandos")?;
    set_poll_timeout(&sock)?;
    let state_sock = bind_udp(cfg.state_port, "estado")?;
    set_poll_timeout(&state_sock)?;
    let video_sock = bind_udp(cfg.video_port, "vídeo")?;
    set_poll_timeout(&video_sock)?;

    let (tx, rx) = mpsc::channel();
    let inner = Arc::new(Inner {
        shared,
        sock,
        drone: SocketAddr::new(cfg.drone_ip, cfg.command_port),
        responses: Mutex::new(rx),
        cmd_lock: Mutex::new(()),
        paused: AtomicBool::new(false),
        caps: Mutex::new(capabilities(None, None)),
        video: Mutex::new(VideoWatch::default()),
        video_level: Mutex::new(None),
    });

    let driver = SdkDriver { inner: inner.clone(), threads: Mutex::new(Vec::new()) };
    driver.spawn("tello-sdk-resp", {
        let inner = inner.clone();
        let sock = inner.sock.try_clone()?;
        move || inner.response_loop(sock, tx)
    });
    driver.spawn("tello-sdk-state", {
        let inner = inner.clone();
        move || inner.state_loop(state_sock)
    });
    driver.spawn("tello-sdk-video", {
        let inner = inner.clone();
        move || inner.video_loop(video_sock)
    });

    if let Err(e) = inner.handshake() {
        driver.shutdown();
        return Err(e);
    }
    inner.shared.mark_connected();
    driver.spawn("tello-sdk-control", {
        let inner = inner.clone();
        move || inner.control_loop()
    });
    driver.spawn("tello-sdk-poll", {
        let inner = inner.clone();
        move || inner.poll_loop()
    });
    Ok(driver)
}

impl SdkDriver {
    fn spawn(&self, name: &str, f: impl FnOnce() + Send + 'static) {
        let handle = thread::Builder::new().name(name.into()).spawn(f).expect("falha ao criar thread");
        self.threads.lock().unwrap().push(handle);
    }
}

impl Driver for SdkDriver {
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

impl Drop for SdkDriver {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Inner {
    fn handshake(&self) -> Result<(), Error> {
        let enter = SdkCommand { retries: 3, ..SdkCommand::query("command") };
        self.send(&enter).map_err(|e| match e {
            Error::Timeout(_) => Error::Timeout(
                "o drone não respondeu ao 'command'. Confira se o computador está conectado \
                 ao Wi-Fi TELLO-XXXXXX e se o drone está ligado."
                    .into(),
            ),
            other => other,
        })?;
        let sdk = self.send(&SdkCommand::query("sdk?")).ok().and_then(|r| parse_sdk_version(&r));
        let serial = self.send(&SdkCommand::query("sn?")).ok().filter(|s| !s.is_empty());
        *self.caps.lock().unwrap() = capabilities(sdk, serial);

        let battery = self.send(&SdkCommand::query("battery?")).ok().and_then(|r| r.parse::<u8>().ok());
        let wifi = self.send(&SdkCommand::query("wifi?")).ok().and_then(|r| r.parse::<u8>().ok());
        if battery.is_some() || wifi.is_some() {
            self.shared.patch_telemetry(|t| {
                t.battery = battery.or(t.battery);
                t.wifi_strength = wifi.or(t.wifi_strength);
            });
        }
        Ok(())
    }

    fn action(&self, action: &Action) -> Result<String, Error> {
        let caps = self.caps.lock().unwrap().clone();
        let cmd = commands::translate(action, &caps)?;
        if matches!(action, Action::Emergency) {
            // Sem fila nem espera: repete para garantir a entrega.
            for _ in 0..3 {
                self.send_raw(&cmd.text);
            }
            self.shared.set_flying(false);
            return Ok("ok".into());
        }
        let result = self.send(&cmd);
        match (cmd.text.as_str(), &result) {
            // Em caso de timeout na decolagem, assume que pode estar voando.
            ("takeoff", Ok(_) | Err(Error::Timeout(_))) => self.shared.set_flying(true),
            ("land", Ok(_)) => self.shared.set_flying(false),
            ("streamon", Ok(_)) => {
                let mut v = self.video.lock().unwrap();
                *v = VideoWatch { stream_on_at: Some(Instant::now()), ..Default::default() };
            }
            ("streamoff", Ok(_)) => self.video.lock().unwrap().stream_on_at = None,
            _ => {}
        }
        result
    }

    fn send_raw(&self, text: &str) {
        match self.sock.send_to(text.as_bytes(), self.drone) {
            Ok(_) => self.shared.blackbox.record(Dir::Tx, Channel::Cmd, text, None),
            Err(e) => self.shared.blackbox.info(Channel::Cmd, format!("falha ao enviar '{text}': {e}")),
        }
    }

    /// Envia um comando e espera a resposta, com retentativas conforme o tipo.
    fn send(&self, cmd: &SdkCommand) -> Result<String, Error> {
        if cmd.fire_and_forget {
            self.send_raw(&cmd.text);
            return Ok("ok".into());
        }
        let _lock = self.cmd_lock.lock().unwrap();
        let _pause = (cmd.kind == Kind::Flight).then(|| PauseGuard::new(&self.paused));
        let rx = self.responses.lock().unwrap();
        for _attempt in 0..=cmd.retries {
            while rx.try_recv().is_ok() {}
            self.send_raw(&cmd.text);
            let deadline = Instant::now() + cmd.timeout;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match rx.recv_timeout(remaining) {
                    // Um "ok" perdido de outro comando não responde a uma consulta.
                    Ok(resp) if cmd.kind == Kind::Query && cmd.text.ends_with('?') && resp == "ok" => continue,
                    Ok(resp) => return classify(&resp),
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return Err(Error::NotConnected),
                }
            }
            if !self.shared.running() {
                return Err(Error::NotConnected);
            }
        }
        Err(Error::Timeout(cmd.text.clone()))
    }

    /// Consulta que desiste se outro comando estiver em andamento.
    fn try_query(&self, text: &str) -> Option<String> {
        let _lock = self.cmd_lock.try_lock().ok()?;
        let rx = self.responses.lock().unwrap();
        while rx.try_recv().is_ok() {}
        self.send_raw(text);
        let deadline = Instant::now() + POLL_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining) {
                Ok(resp) if resp == "ok" => continue,
                Ok(resp) => return Some(resp),
                Err(_) => return None,
            }
        }
    }

    fn poll_loop(&self) {
        let mut last = Instant::now();
        let mut applied: Option<VideoPolicy> = None;
        while self.shared.running() {
            thread::sleep(Duration::from_millis(100));
            self.apply_video_policy(&mut applied);
            if last.elapsed() < WIFI_POLL || self.paused.load(Ordering::Relaxed) {
                continue;
            }
            last = Instant::now();
            if let Some(w) = self.try_query("wifi?").and_then(|r| r.parse::<u8>().ok()) {
                self.shared.patch_telemetry(|t| t.wifi_strength = Some(w));
            }
        }
    }

    /// Sem a perda de pacotes o modo adaptativo não funciona aqui: só o nível
    /// fixo é aplicado, e só no SDK 3, que tem `setbitrate`.
    fn apply_video_policy(&self, applied: &mut Option<VideoPolicy>) {
        let policy = self.shared.video_policy();
        if *applied == Some(policy) {
            return;
        }
        let first = applied.is_none();
        *applied = Some(policy);
        let caps = self.caps.lock().unwrap().clone();
        if !caps.video_bitrate || policy.adaptive || (first && policy.level == 0) {
            return;
        }
        let level = policy.level;
        let result = commands::translate(&Action::SetVideoBitrate { level }, &caps).and_then(|c| self.send(&c));
        match result {
            Ok(_) => *self.video_level.lock().unwrap() = (level != 0).then_some(level),
            Err(e) => self.shared.blackbox.info(Channel::Cmd, format!("taxa do vídeo não aplicada: {e}")),
        }
    }

    fn response_loop(&self, sock: UdpSocket, tx: Sender<String>) {
        let mut buf = [0u8; 2048];
        while self.shared.running() {
            match sock.recv_from(&mut buf) {
                Ok((n, from)) if from.ip() == self.drone.ip() => {
                    let text = String::from_utf8_lossy(&buf[..n]).trim().to_string();
                    self.shared.blackbox.record(Dir::Rx, Channel::Cmd, text.clone(), None);
                    let _ = tx.send(text);
                }
                Ok(_) => {}
                Err(e) if is_timeout(&e) => {}
                Err(e) => {
                    // No Windows, um ICMP "porta inalcançável" aparece como erro no recv.
                    self.shared.blackbox.info(Channel::Cmd, format!("erro no recv: {e}"));
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }

    fn state_loop(&self, sock: UdpSocket) {
        let mut buf = [0u8; 2048];
        while self.shared.running() {
            match sock.recv_from(&mut buf) {
                Ok((n, _)) => {
                    let text = String::from_utf8_lossy(&buf[..n]);
                    let text = text.trim();
                    self.shared.blackbox.record(Dir::Rx, Channel::State, text, None);
                    let mut parsed = 0;
                    let flying = self.shared.is_flying();
                    self.shared.update_telemetry(|t| {
                        parsed = parse_state(text, t);
                        // O estado do SDK não diz se está voando: vem dos comandos.
                        t.flying = Some(flying);
                    });
                    if parsed == 0 {
                        self.shared.blackbox.info(Channel::State, "estado não reconhecido");
                    }
                }
                Err(e) if is_timeout(&e) => {}
                Err(_) => thread::sleep(Duration::from_millis(100)),
            }
        }
    }

    fn video_loop(&self, sock: UdpSocket) {
        let mut buf = vec![0u8; 4096];
        let mut asm = NalAssembler::new();
        let mut stats = VideoWindow::default();
        while self.shared.running() {
            match sock.recv_from(&mut buf) {
                Ok((n, _)) => {
                    self.video.lock().unwrap().last_packet = Some(Instant::now());
                    let data = &buf[..n];
                    let mut frames = asm.push(data);
                    if n < VIDEO_CHUNK {
                        frames.extend(asm.flush());
                    }
                    stats.packets += 1;
                    stats.bytes += n;
                    for f in frames {
                        stats.frames += 1;
                        stats.keyframes += f.keyframe as usize;
                        self.shared.emit(DroneEvent::Video(f));
                    }
                }
                Err(e) if is_timeout(&e) => {}
                Err(_) => thread::sleep(Duration::from_millis(100)),
            }
            if stats.since.elapsed() >= Duration::from_secs(1) {
                let streaming = self.video.lock().unwrap().stream_on_at.is_some();
                stats.close(&self.shared, &asm, streaming.then(|| *self.video_level.lock().unwrap()));
                stats = VideoWindow::default();
            }
        }
    }

    fn control_loop(self: &Arc<Self>) {
        let mut last_rc = String::new();
        let mut last_rc_log = Instant::now();
        let mut last_check = Instant::now();
        let mut next = Instant::now();
        while self.shared.running() {
            if self.shared.take_auto_land() {
                self.spawn_land();
            }
            if !self.paused.load(Ordering::Relaxed) {
                let text = commands::rc(self.shared.effective_sticks());
                if let Err(e) = self.sock.send_to(text.as_bytes(), self.drone) {
                    self.shared.blackbox.info(Channel::Cmd, format!("falha no rc: {e}"));
                }
                // Loga só mudanças e um "batimento" por segundo, para não poluir.
                if text != last_rc || last_rc_log.elapsed() >= Duration::from_secs(1) {
                    self.shared.blackbox.record(Dir::Tx, Channel::Cmd, text.clone(), None);
                    last_rc_log = Instant::now();
                    last_rc = text;
                }
            }
            if last_check.elapsed() >= Duration::from_secs(1) {
                last_check = Instant::now();
                self.shared.check_link(NO_STATE_HINT);
                self.check_video();
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

    fn spawn_land(self: &Arc<Self>) {
        let inner = self.clone();
        thread::spawn(move || {
            if let Err(e) = inner.action(&Action::Land) {
                inner.shared.blackbox.info(Channel::Core, format!("pouso automático falhou: {e}"));
            }
        });
    }

    fn check_video(&self) {
        let mut v = self.video.lock().unwrap();
        let Some(on_at) = v.stream_on_at else { return };
        if !v.warned && v.last_packet.is_none() && on_at.elapsed() > NO_VIDEO_AFTER {
            v.warned = true;
            drop(v);
            self.shared.warn(WarningCode::NoVideo, NO_VIDEO_HINT);
        }
    }
}

/// Pausa o `rc` enquanto vive.
struct PauseGuard<'a>(&'a AtomicBool);

impl<'a> PauseGuard<'a> {
    fn new(flag: &'a AtomicBool) -> Self {
        flag.store(true, Ordering::Relaxed);
        PauseGuard(flag)
    }
}

impl Drop for PauseGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// Contagem do vídeo em uma janela de 1 s.
struct VideoWindow {
    since: Instant,
    packets: usize,
    bytes: usize,
    frames: usize,
    keyframes: usize,
}

impl Default for VideoWindow {
    fn default() -> Self {
        VideoWindow { since: Instant::now(), packets: 0, bytes: 0, frames: 0, keyframes: 0 }
    }
}

impl VideoWindow {
    /// Registra a janela na caixa-preta e, com o vídeo ligado (`streaming` com
    /// o nível de taxa), publica o resumo.
    fn close(&self, shared: &Shared, asm: &NalAssembler, streaming: Option<Option<u8>>) {
        if let Some(level) = streaming {
            shared.emit(DroneEvent::VideoStats(VideoStats {
                fps: self.frames.min(u16::MAX as usize) as u16,
                kbps: (self.bytes * 8 / 1000) as u32,
                loss_pct: None,
                level,
            }));
        }
        if self.packets > 0 {
            shared.blackbox.record(
                Dir::Rx,
                Channel::Video,
                format!(
                    "{} pacotes, {} KB, {} frames ({} chave), {} descartados",
                    self.packets,
                    self.bytes / 1024,
                    self.frames,
                    self.keyframes,
                    asm.dropped()
                ),
                None,
            );
        }
    }
}
