//! Simulador de Tello: responde ao SDK como o drone real, envia estado a
//! 10 Hz e transmite um vídeo H.264 de teste. Serve para desenvolver a UI e
//! para os testes de integração, sem precisar do drone.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tello_core::NalAssembler;

mod app;

/// Vídeo de teste 960x720 a 30 fps (H.264 Main, gerado uma vez com ffmpeg).
pub const FIXTURE_H264: &[u8] = include_bytes!("../fixtures/sample.h264");
/// Foto de teste enviada no lugar da foto de 5 MP no modo App.
pub const FIXTURE_JPEG: &[u8] = include_bytes!("../fixtures/photo.jpg");
const VIDEO_CHUNK: usize = 1460;

#[derive(Clone, Debug)]
pub struct SimConfig {
    /// Endereço de comandos (porta 0 = qualquer livre).
    pub bind: SocketAddr,
    /// Porta do cliente que recebe o estado.
    pub state_port: u16,
    /// Porta do cliente que recebe o vídeo.
    pub video_port: u16,
    /// Resposta de `sdk?` ("20", "30"); `None` simula o SDK 1.3.
    pub sdk_version: Option<String>,
    pub serial: String,
    pub battery: u8,
    /// Multiplica a duração das manobras (testes usam valores pequenos).
    pub time_scale: f32,
    /// Pouso automático sem comandos, como o drone real (15 s).
    pub auto_land_after: Duration,
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8889),
            state_port: 8890,
            video_port: 11111,
            sdk_version: Some("20".into()),
            serial: "0TQZSIM0000001".into(),
            battery: 87,
            time_scale: 1.0,
            auto_land_after: Duration::from_secs(15),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct State {
    sdk_mode: bool,
    client: Option<SocketAddr>,
    flying: bool,
    height_cm: f32,
    yaw_deg: f32,
    battery: f32,
    streaming: bool,
    speed: u16,
    last_cmd: Instant,
    rc: [i32; 4],
    motor_time_s: f32,
    auto_landed: bool,
    /// Conectado pelo protocolo do app (conn_req) em vez do SDK.
    app_mode: bool,
    app_video_port: u16,
    fast: bool,
}

pub(crate) struct Inner {
    cfg: SimConfig,
    sock: UdpSocket,
    running: AtomicBool,
    state: Mutex<State>,
    received: Mutex<Vec<String>>,
    verbose: bool,
}

pub struct Sim {
    inner: Arc<Inner>,
    threads: Vec<JoinHandle<()>>,
}

impl Sim {
    pub fn start(cfg: SimConfig) -> io::Result<Sim> {
        Self::start_inner(cfg, false)
    }

    /// Como [`Sim::start`], imprimindo cada comando recebido.
    pub fn start_verbose(cfg: SimConfig) -> io::Result<Sim> {
        Self::start_inner(cfg, true)
    }

    fn start_inner(cfg: SimConfig, verbose: bool) -> io::Result<Sim> {
        let sock = UdpSocket::bind(cfg.bind)?;
        sock.set_read_timeout(Some(Duration::from_millis(100)))?;
        let state = State {
            sdk_mode: false,
            client: None,
            flying: false,
            height_cm: 0.0,
            yaw_deg: 0.0,
            battery: cfg.battery as f32,
            streaming: false,
            speed: 100,
            last_cmd: Instant::now(),
            rc: [0; 4],
            motor_time_s: 0.0,
            auto_landed: false,
            app_mode: false,
            app_video_port: 0,
            fast: false,
        };
        let inner = Arc::new(Inner {
            cfg,
            sock,
            running: AtomicBool::new(true),
            state: Mutex::new(state),
            received: Mutex::new(Vec::new()),
            verbose,
        });
        let mut threads = Vec::new();
        for (name, f) in [
            ("sim-cmd", Inner::command_loop as fn(&Arc<Inner>)),
            ("sim-state", Inner::state_loop),
            ("sim-video", Inner::video_loop),
        ] {
            let inner = inner.clone();
            threads.push(thread::Builder::new().name(name.into()).spawn(move || f(&inner))?);
        }
        Ok(Sim { inner, threads })
    }

    pub fn addr(&self) -> SocketAddr {
        self.inner.sock.local_addr().expect("socket do simulador")
    }

    /// Todos os comandos recebidos, exceto `rc`.
    pub fn received(&self) -> Vec<String> {
        self.inner.received.lock().unwrap().clone()
    }

    pub fn last_rc(&self) -> [i32; 4] {
        self.inner.state.lock().unwrap().rc
    }

    pub fn flying(&self) -> bool {
        self.inner.state.lock().unwrap().flying
    }

    pub fn height_cm(&self) -> f32 {
        self.inner.state.lock().unwrap().height_cm
    }

    pub fn auto_landed(&self) -> bool {
        self.inner.state.lock().unwrap().auto_landed
    }

    pub fn streaming(&self) -> bool {
        self.inner.state.lock().unwrap().streaming
    }

    pub fn app_mode(&self) -> bool {
        self.inner.state.lock().unwrap().app_mode
    }

    pub fn fast_mode(&self) -> bool {
        self.inner.state.lock().unwrap().fast
    }

    pub fn set_battery(&self, pct: u8) {
        self.inner.state.lock().unwrap().battery = pct as f32;
    }

    pub fn stop(&mut self) {
        self.inner.running.store(false, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

impl Drop for Sim {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Inner {
    fn running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    fn scaled(&self, secs: f32) -> Duration {
        Duration::from_secs_f32((secs * self.cfg.time_scale).max(0.0))
    }

    fn reply(&self, to: SocketAddr, text: &str) {
        let _ = self.sock.send_to(text.as_bytes(), to);
    }

    /// Responde depois de `secs` (escalado), aplicando `effect` ao estado.
    fn reply_later(self: &Arc<Self>, to: SocketAddr, secs: f32, text: &'static str, effect: fn(&mut State)) {
        let inner = self.clone();
        let delay = self.scaled(secs);
        thread::spawn(move || {
            thread::sleep(delay);
            effect(&mut inner.state.lock().unwrap());
            inner.reply(to, text);
        });
    }

    fn command_loop(self: &Arc<Self>) {
        let mut buf = [0u8; 2048];
        while self.running() {
            let Ok((n, from)) = self.sock.recv_from(&mut buf) else { continue };
            let data = &buf[..n];
            let app = self.state.lock().unwrap().app_mode;
            if data.starts_with(b"conn_req:") || data.first() == Some(&0xcc) || (app && data == b"emergency") {
                self.handle_app(data, from);
            } else {
                let text = String::from_utf8_lossy(data).trim().to_string();
                self.handle(&text, from);
            }
        }
    }

    fn handle(self: &Arc<Self>, text: &str, from: SocketAddr) {
        let mut words = text.split_whitespace();
        let cmd = words.next().unwrap_or_default();
        let arg = words.next().and_then(|w| w.parse::<i32>().ok());

        let mut st = self.state.lock().unwrap();
        if cmd == "command" {
            st.sdk_mode = true;
            st.client = Some(from);
        }
        if !st.sdk_mode {
            return; // o drone real ignora tudo antes de "command"
        }
        st.last_cmd = Instant::now();
        if cmd == "rc" {
            let v: Vec<i32> = text.split_whitespace().skip(1).filter_map(|w| w.parse().ok()).collect();
            if v.len() == 4 {
                st.rc = [v[0], v[1], v[2], v[3]];
            }
            return; // rc não tem resposta
        }
        if self.verbose {
            println!("<- {text}");
        }
        self.received.lock().unwrap().push(text.to_string());
        let flying = st.flying;
        let battery = st.battery;
        drop(st);

        match cmd {
            "command" => self.reply(from, "ok"),
            "takeoff" if flying => self.reply(from, "error Motor stop"),
            "takeoff" if battery < 10.0 => self.reply(from, "error battery low"),
            "takeoff" => self.reply_later(from, 2.0, "ok", |s| {
                s.flying = true;
                s.height_cm = 80.0;
                s.auto_landed = false;
            }),
            "land" => self.reply_later(from, 2.0, "ok", |s| {
                s.flying = false;
                s.height_cm = 0.0;
            }),
            "emergency" => {
                let mut s = self.state.lock().unwrap();
                s.flying = false;
                s.height_cm = 0.0;
                drop(s);
                self.reply(from, "ok");
            }
            "streamon" => {
                self.state.lock().unwrap().streaming = true;
                self.reply(from, "ok");
            }
            "streamoff" => {
                self.state.lock().unwrap().streaming = false;
                self.reply(from, "ok");
            }
            "stop" if self.cfg.sdk_version.is_some() => {
                self.state.lock().unwrap().rc = [0; 4];
                self.reply(from, "ok");
            }
            "speed" => match arg {
                Some(v @ 10..=100) => {
                    self.state.lock().unwrap().speed = v as u16;
                    self.reply(from, "ok");
                }
                _ => self.reply(from, "out of range"),
            },
            "up" | "down" | "left" | "right" | "forward" | "back" | "cw" | "ccw" | "flip" if !flying => {
                self.reply(from, "error Not joystick")
            }
            "flip" if battery < 50.0 => self.reply(from, "error No valid imu"),
            "flip" => self.reply_later(from, 1.5, "ok", noop),
            "up" => match arg {
                Some(20..=500) => {
                    let mut s = self.state.lock().unwrap();
                    s.height_cm += arg.unwrap() as f32;
                    drop(s);
                    self.reply_later(from, 1.0, "ok", noop);
                }
                _ => self.reply(from, "out of range"),
            },
            "down" => match arg {
                Some(20..=500) => {
                    let mut s = self.state.lock().unwrap();
                    s.height_cm = (s.height_cm - arg.unwrap() as f32).max(20.0);
                    drop(s);
                    self.reply_later(from, 1.0, "ok", noop);
                }
                _ => self.reply(from, "out of range"),
            },
            "left" | "right" | "forward" | "back" => match arg {
                Some(20..=500) => self.reply_later(from, 1.0, "ok", noop),
                _ => self.reply(from, "out of range"),
            },
            "cw" | "ccw" => match arg {
                Some(d @ 1..=360) => {
                    let mut s = self.state.lock().unwrap();
                    s.yaw_deg += if cmd == "cw" { d as f32 } else { -(d as f32) };
                    drop(s);
                    self.reply_later(from, 1.0, "ok", noop);
                }
                _ => self.reply(from, "out of range"),
            },
            "battery?" => self.reply(from, &format!("{}", battery.round() as i32)),
            "speed?" => self.reply(from, &format!("{}.0", self.state.lock().unwrap().speed)),
            "time?" => self.reply(from, &format!("{}s", self.state.lock().unwrap().motor_time_s as i32)),
            "height?" => self.reply(from, &format!("{}dm", (self.state.lock().unwrap().height_cm / 10.0) as i32)),
            "wifi?" => self.reply(from, "90"),
            "sdk?" => match &self.cfg.sdk_version {
                Some(v) => self.reply(from, v),
                None => self.reply(from, "unknown command: sdk?"),
            },
            "sn?" => self.reply(from, &self.cfg.serial),
            _ => self.reply(from, &format!("unknown command: {text}")),
        }
    }

    fn state_loop(self: &Arc<Self>) {
        let out = match UdpSocket::bind((self.cfg.bind.ip(), 0)) {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut last = Instant::now();
        let mut tick = 0u32;
        while self.running() {
            thread::sleep(Duration::from_millis(100));
            tick = tick.wrapping_add(1);
            let dt = last.elapsed().as_secs_f32();
            last = Instant::now();
            let (text, client) = {
                let mut s = self.state.lock().unwrap();
                if s.flying {
                    let climb = s.rc[2] as f32 / 100.0 * 100.0; // até 1 m/s
                    s.height_cm = (s.height_cm + climb * dt).clamp(10.0, 1000.0);
                    s.yaw_deg += s.rc[3] as f32 / 100.0 * 90.0 * dt;
                    s.motor_time_s += dt;
                    s.battery = (s.battery - dt / 20.0).max(0.0);
                    if s.last_cmd.elapsed() > self.cfg.auto_land_after {
                        s.flying = false;
                        s.height_cm = 0.0;
                        s.auto_landed = true;
                    }
                }
                if s.app_mode {
                    let snapshot = s.clone();
                    drop(s);
                    self.app_telemetry(&snapshot, tick);
                    continue;
                }
                (state_string(&s, self.cfg.sdk_version.is_some()), s.client)
            };
            if let Some(c) = client {
                let _ = out.send_to(text.as_bytes(), (c.ip(), self.cfg.state_port));
            }
        }
    }

    fn video_loop(self: &Arc<Self>) {
        let out = match UdpSocket::bind((self.cfg.bind.ip(), 0)) {
            Ok(s) => s,
            Err(_) => return,
        };
        let frames = fixture_frames();
        let mut i = 0;
        let frame_time = Duration::from_micros(33_333);
        let mut next = Instant::now();
        while self.running() {
            let (streaming, client, app, app_port) = {
                let s = self.state.lock().unwrap();
                (s.streaming, s.client, s.app_mode, s.app_video_port)
            };
            match (streaming, client) {
                (true, Some(c)) if !frames.is_empty() => {
                    let frame = &frames[i % frames.len()];
                    if app {
                        // Modo App: cabeçalho de 2 bytes (frame, fragmento | 0x80 no último).
                        let chunks: Vec<_> = frame.chunks(VIDEO_CHUNK - 2).collect();
                        for (k, chunk) in chunks.iter().enumerate() {
                            let last = if k + 1 == chunks.len() { 0x80 } else { 0 };
                            let mut pkt = vec![i as u8, k as u8 | last];
                            pkt.extend_from_slice(chunk);
                            let _ = out.send_to(&pkt, (c.ip(), app_port));
                        }
                    } else {
                        for chunk in frame.chunks(VIDEO_CHUNK) {
                            let _ = out.send_to(chunk, (c.ip(), self.cfg.video_port));
                        }
                    }
                    i += 1;
                    next += frame_time;
                    let now = Instant::now();
                    if next > now {
                        thread::sleep(next - now);
                    } else {
                        next = now;
                    }
                }
                _ => {
                    i = 0;
                    thread::sleep(Duration::from_millis(50));
                    next = Instant::now();
                }
            }
        }
    }
}

fn noop(_: &mut State) {}

/// Divide o vídeo de teste em frames (access units) Annex B.
pub fn fixture_frames() -> Vec<Vec<u8>> {
    let mut asm = NalAssembler::new();
    let mut frames: Vec<Vec<u8>> = asm.push(FIXTURE_H264).into_iter().map(|f| f.data).collect();
    frames.extend(asm.flush().into_iter().map(|f| f.data));
    frames
}

fn state_string(s: &State, edu: bool) -> String {
    let yaw = ((s.yaw_deg + 180.0).rem_euclid(360.0) - 180.0).round() as i32;
    let (pitch, roll) = if s.flying { (s.rc[1] / 10, s.rc[0] / 10) } else { (0, 0) };
    let tof = if s.flying { s.height_cm as i32 + 10 } else { 10 };
    let pad = if edu { "mid:-1;x:0;y:0;z:0;mpry:0,0,0;" } else { "" };
    format!(
        "{pad}pitch:{pitch};roll:{roll};yaw:{yaw};vgx:{};vgy:{};vgz:{};templ:62;temph:65;tof:{tof};h:{};bat:{};\
         baro:{:.2};time:{};agx:-3.00;agy:2.00;agz:-998.00;\r\n",
        s.rc[1] / 20,
        s.rc[0] / 20,
        -s.rc[2] / 20,
        if s.flying { s.height_cm as i32 } else { 0 },
        s.battery.round() as i32,
        100.0 + s.height_cm / 100.0,
        s.motor_time_s as i32,
    )
}
