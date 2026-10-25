//! Testes de integração do protocolo do app: cliente contra o simulador.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use tello_core::{
    Action, CalibrationState, Condition, Config, Drone, DroneEvent, Error, FlipDir, Protocol, Safety, SmartVideo,
    Sticks, VideoPolicy, WarningCode,
};
use tello_sim::{Sim, SimConfig, FIXTURE_JPEG};

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

fn free_port() -> u16 {
    UdpSocket::bind((LOCALHOST, 0)).unwrap().local_addr().unwrap().port()
}

struct Rig {
    sim: Sim,
    drone: Drone,
    events: Arc<Mutex<Vec<DroneEvent>>>,
    photo_dir: PathBuf,
}

impl Rig {
    fn connect() -> Rig {
        Self::connect_with(|_| {})
    }

    fn connect_with(sim_cfg: impl FnOnce(&mut SimConfig)) -> Rig {
        let mut cfg = SimConfig { bind: SocketAddr::new(LOCALHOST, 0), time_scale: 0.05, ..Default::default() };
        sim_cfg(&mut cfg);
        let sim = Sim::start(cfg).unwrap();
        let photo_dir = std::env::temp_dir().join(format!("tello-test-{}", free_port()));
        let config = Config {
            drone_ip: LOCALHOST,
            command_port: sim.addr().port(),
            app_video_port: free_port(),
            photo_dir: Some(photo_dir.clone()),
            report_dir: Some(photo_dir.clone()),
            ..Default::default()
        };
        let events: Arc<Mutex<Vec<DroneEvent>>> = Arc::default();
        let sink = {
            let events = events.clone();
            Arc::new(move |e| events.lock().unwrap().push(e))
        };
        let drone = Drone::connect(config, Protocol::App, sink).expect("conexão no modo App");
        Rig { sim, drone, events, photo_dir }
    }

    fn wait_for<T>(&self, what: &str, timeout: Duration, f: impl Fn(&DroneEvent) -> Option<T>) -> T {
        let start = Instant::now();
        loop {
            if let Some(v) = self.events.lock().unwrap().iter().find_map(&f) {
                return v;
            }
            assert!(start.elapsed() < timeout, "não chegou: {what}");
            sleep(Duration::from_millis(20));
        }
    }

    fn wait_flying(&self, flying: bool) {
        let start = Instant::now();
        loop {
            let last = self.events.lock().unwrap().iter().rev().find_map(|e| {
                if let DroneEvent::Telemetry { data } = e {
                    data.flying
                } else {
                    None
                }
            });
            if last == Some(flying) {
                return;
            }
            assert!(start.elapsed() < Duration::from_secs(3), "telemetria não indicou voando={flying}");
            sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.photo_dir);
    }
}

fn wait_until(what: &str, cond: impl Fn() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < Duration::from_secs(3), "condição não atingida: {what}");
        sleep(Duration::from_millis(20));
    }
}

#[test]
fn handshake_capabilities_and_telemetry() {
    let rig = Rig::connect();
    assert!(rig.sim.app_mode());
    let caps = rig.drone.caps();
    assert_eq!(caps.protocol, Protocol::App);
    assert!(caps.flips_diagonal && caps.throw_and_go && caps.palm_land && caps.hi_res_photo);
    rig.wait_for("telemetria do modo App", Duration::from_secs(2), |e| match e {
        DroneEvent::Telemetry { data } if data.battery == Some(87) && data.wifi_strength == Some(90) => Some(()),
        _ => None,
    });
    wait_until("versão do firmware", || rig.drone.caps().firmware.as_deref() == Some("01.04.92.01"));
    for expected in ["conn_req", "app:log_header", "app:version", "app:set_alt_limit"] {
        wait_until(expected, || rig.sim.received().iter().any(|r| r == expected));
    }
}

#[test]
fn takeoff_sticks_fast_mode_and_land() {
    let rig = Rig::connect();
    rig.drone.action(&Action::Takeoff).unwrap();
    rig.wait_flying(true);
    rig.drone.set_safety(Safety::expert());
    rig.drone.set_sticks(Sticks { throttle: 1.0, roll: -0.5, ..Sticks::NEUTRAL });
    wait_until("sticks no simulador", || rig.sim.last_rc() == [-50, 0, 100, 0]);
    rig.drone.action(&Action::SetFastMode { on: true }).unwrap();
    wait_until("modo rápido", || rig.sim.fast_mode());
    rig.drone.action(&Action::Land).unwrap();
    rig.wait_flying(false);
}

#[test]
fn flips_are_refused_on_ground_and_work_in_the_air() {
    let rig = Rig::connect();
    assert!(matches!(rig.drone.action(&Action::Flip { dir: FlipDir::Left }), Err(Error::Refused(_))));
    rig.drone.action(&Action::Takeoff).unwrap();
    rig.wait_flying(true);
    assert_eq!(rig.drone.action(&Action::Flip { dir: FlipDir::BackLeft }).unwrap(), "ok");
    assert_eq!(rig.drone.action(&Action::SmartVideo { mode: SmartVideo::Spin360 }).unwrap(), "ok");
    assert!(matches!(rig.drone.action(&Action::Raw { text: "battery?".into() }), Err(Error::NotSupported(_))));
}

#[test]
fn throw_and_go_then_palm_land() {
    let rig = Rig::connect();
    rig.drone.action(&Action::ThrowAndGo).unwrap();
    rig.wait_flying(true);
    rig.drone.action(&Action::PalmLand).unwrap();
    rig.wait_flying(false);
}

#[test]
fn downloads_high_resolution_photo() {
    let rig = Rig::connect();
    rig.drone.action(&Action::TakePhoto).unwrap();
    let path = rig.wait_for("foto salva", Duration::from_secs(5), |e| match e {
        DroneEvent::PhotoSaved { path } => Some(path.clone()),
        _ => None,
    });
    assert_eq!(std::fs::read(&path).unwrap(), FIXTURE_JPEG);
    let received = rig.sim.received();
    let chunk_acks = received.iter().filter(|r| *r == "app:file_data").count();
    assert_eq!(chunk_acks, FIXTURE_JPEG.len().div_ceil(8 * 1024), "um ack por chunk");
    assert!(received.iter().any(|r| r == "app:file_size") && received.iter().any(|r| r == "app:file_complete"));
}

#[test]
fn streams_video_with_two_byte_header() {
    let rig = Rig::connect();
    rig.drone.action(&Action::StreamOn).unwrap();
    wait_until("20 frames de vídeo", || {
        rig.events.lock().unwrap().iter().filter(|e| matches!(e, DroneEvent::Video(_))).count() >= 20
    });
    let events = rig.events.lock().unwrap();
    let first = events.iter().find_map(|e| if let DroneEvent::Video(f) = e { Some(f) } else { None }).unwrap();
    assert!(first.keyframe && first.has_sps);
}

#[test]
fn attitude_comes_from_imu_log() {
    let rig = Rig::connect();
    rig.drone.action(&Action::Takeoff).unwrap();
    rig.wait_flying(true);
    rig.drone.set_safety(Safety::expert());
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(700) {
        rig.drone.set_sticks(Sticks { yaw: 1.0, ..Sticks::NEUTRAL });
        sleep(Duration::from_millis(30));
    }
    rig.wait_for("yaw girando", Duration::from_secs(2), |e| match e {
        DroneEvent::Telemetry { data } if data.yaw_deg.unwrap_or(0) > 20 => Some(()),
        _ => None,
    });
}

#[test]
fn emergency_stops_motors() {
    let rig = Rig::connect();
    rig.drone.action(&Action::Takeoff).unwrap();
    rig.wait_flying(true);
    rig.drone.action(&Action::Emergency).unwrap();
    wait_until("motores desligados", || !rig.sim.flying());
}

#[test]
fn warns_about_poor_light_and_wind() {
    let rig = Rig::connect();
    rig.wait_for("luz informada", Duration::from_secs(2), |e| match e {
        DroneEvent::Telemetry { data } if data.light_strength == Some(0) => Some(()),
        _ => None,
    });
    rig.sim.set_light_poor(true);
    rig.wait_for("aviso de pouca luz", Duration::from_secs(6), |e| match e {
        DroneEvent::Warning { code: WarningCode::LowLight, .. } => Some(()),
        _ => None,
    });
    // Vento só conta com o drone no ar.
    rig.sim.set_wind(true);
    rig.drone.action(&Action::Takeoff).unwrap();
    rig.wait_flying(true);
    rig.wait_for("condições com vento", Duration::from_secs(6), |e| match e {
        DroneEvent::Conditions { active } if active == &[Condition::LowLight, Condition::Wind] => Some(()),
        _ => None,
    });
    rig.sim.set_light_poor(false);
    rig.wait_for("luz normalizada", Duration::from_secs(6), |e| match e {
        DroneEvent::Conditions { active } if active == &[Condition::Wind] => Some(()),
        _ => None,
    });
    let t = rig.drone.telemetry();
    assert_eq!((t.wind, t.wifi_interference), (Some(true), Some(0)));
}

#[test]
fn adapts_video_bitrate_to_packet_loss() {
    let rig = Rig::connect();
    rig.drone.action(&Action::StreamOn).unwrap();
    wait_until("taxa inicial no teto", || rig.sim.video_rate() == Some(4));
    rig.wait_for("resumo do vídeo sem perda", Duration::from_secs(3), |e| match e {
        DroneEvent::VideoStats(s) if s.fps > 0 && s.loss_pct == Some(0.0) && s.level == Some(4) => Some(()),
        _ => None,
    });
    let keyframe_requests = || rig.sim.received().iter().filter(|r| *r == "app:video_sps_pps").count();
    let before = keyframe_requests();
    rig.sim.set_video_loss(30);
    sleep(Duration::from_secs(2));
    // Sem perda o pedido é 1 por segundo; com perda, logo depois de cada falha.
    let requests = keyframe_requests() - before;
    assert!(requests >= 5, "só {requests} pedidos de keyframe em 2 s");
    rig.wait_for("taxa reduzida", Duration::from_secs(6), |e| match e {
        DroneEvent::VideoStats(s) if s.level == Some(3) && s.loss_pct.is_some_and(|l| l > 5.0) => Some(()),
        _ => None,
    });
    assert_eq!(rig.sim.video_rate(), Some(3));
}

#[test]
fn fixed_video_rate_follows_the_policy() {
    let rig = Rig::connect();
    wait_until("taxa inicial", || rig.sim.video_rate() == Some(4));
    rig.drone.set_video_policy(VideoPolicy { adaptive: false, level: 2 });
    wait_until("taxa fixa", || rig.sim.video_rate() == Some(2));
    rig.drone.set_video_policy(VideoPolicy { adaptive: false, level: 0 });
    wait_until("taxa automática do drone", || rig.sim.video_rate() == Some(0));
}

/// Índice do evento logo depois da `n`-ésima conexão (1 = a primeira).
fn after_connected(events: &[DroneEvent], n: usize) -> Option<usize> {
    events.iter().enumerate().filter(|(_, e)| matches!(e, DroneEvent::Connected { .. })).nth(n - 1).map(|(i, _)| i + 1)
}

#[test]
fn reconnects_after_the_link_drops() {
    let rig = Rig::connect_with(|c| c.auto_land_after = Duration::from_secs(120));
    rig.drone.action(&Action::Takeoff).unwrap();
    rig.wait_flying(true);
    rig.drone.action(&Action::StreamOn).unwrap();
    rig.drone.action(&Action::SetFastMode { on: true }).unwrap();
    wait_until("modo rápido nos sticks", || rig.sim.fast_mode());

    rig.sim.set_offline(true);
    rig.wait_for("aviso de link perdido", Duration::from_secs(5), |e| match e {
        DroneEvent::Warning { code: WarningCode::LinkLost, .. } => Some(()),
        _ => None,
    });
    rig.wait_for("início da reconexão", Duration::from_secs(5), |e| match e {
        DroneEvent::Reconnecting { attempt: 1 } => Some(()),
        _ => None,
    });
    assert!(rig.drone.is_reconnecting());
    assert!(matches!(rig.drone.action(&Action::Land), Err(Error::Reconnecting)));
    let handshakes = || rig.sim.received().iter().filter(|r| *r == "conn_req").count();
    let before = handshakes();

    rig.sim.set_offline(false);
    wait_until("nova conexão", || after_connected(&rig.events.lock().unwrap(), 2).is_some());
    assert!(handshakes() > before, "o handshake não foi refeito");
    rig.wait_for("link restabelecido", Duration::from_secs(3), |e| match e {
        DroneEvent::Warning { code: WarningCode::LinkRestored, .. } => Some(()),
        _ => None,
    });
    assert!(!rig.drone.is_reconnecting());
    assert!(rig.drone.is_flying(), "o estado de voo se perdeu na reconexão");
    wait_until("vídeo religado", || {
        let events = rig.events.lock().unwrap();
        let from = after_connected(&events, 2).unwrap();
        events[from..].iter().filter(|e| matches!(e, DroneEvent::Video(_))).count() >= 5
    });
    sleep(Duration::from_millis(200));
    assert!(rig.sim.fast_mode(), "o modo rápido não foi reaplicado");
    rig.drone.action(&Action::Land).unwrap();
}

#[test]
fn disconnecting_in_flight_closes_the_report_as_interrupted() {
    let rig = Rig::connect_with(|c| {
        c.auto_land_after = Duration::from_secs(120);
    });
    rig.sim.set_wind(true);
    rig.drone.action(&Action::Takeoff).unwrap();
    rig.wait_flying(true);
    rig.wait_for("vento", Duration::from_secs(5), |e| match e {
        DroneEvent::Conditions { active } if active.contains(&Condition::Wind) => Some(()),
        _ => None,
    });
    sleep(Duration::from_secs(3));
    rig.drone.disconnect();
    let (path, summary) = rig.wait_for("relatório do voo", Duration::from_secs(1), |e| match e {
        DroneEvent::FlightReport { path, summary } => Some((path.clone(), summary.clone())),
        _ => None,
    });
    assert!(summary.interrupted);
    assert!(summary.causes.iter().any(|c| c.starts_with("Vento forte")), "{:?}", summary.causes);
    assert!(summary.causes.iter().any(|c| c.contains("sem pouso")), "{:?}", summary.causes);
    let html = std::fs::read_to_string(path.expect("caminho")).unwrap();
    assert!(html.contains("interrompido") && html.contains("Linha do tempo"));
    let events = rig.events.lock().unwrap();
    assert!(matches!(events.last(), Some(DroneEvent::Disconnected { .. })), "o relatório vem antes do fim");
}

#[test]
fn calibrates_the_imu_on_the_ground() {
    let rig = Rig::connect();
    assert!(rig.drone.caps().imu_calibration);
    rig.drone.action(&Action::CalibrateImu).unwrap();
    // No simulador o "usuário" leva 30 s × 0,05 = 1,5 s para as 6 posições.
    let done = rig.wait_for("calibração concluída", Duration::from_secs(4), |e| match e {
        DroneEvent::Calibration(c) if c.state == CalibrationState::Done => Some(*c),
        _ => None,
    });
    assert_eq!((done.steps, done.progress), (6, 100));
    let running: Vec<(u8, u8)> = rig
        .events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            DroneEvent::Calibration(c) if c.state == CalibrationState::Running => Some((c.steps, c.progress)),
            _ => None,
        })
        .collect();
    assert!(running.len() >= 3 && running.windows(2).all(|w| w[0] <= w[1]), "{running:?}");
    assert!(rig.sim.received().iter().any(|r| r == "app:calibration_start"));
    let polls = || rig.sim.received().iter().filter(|r| *r == "app:calibration_status").count();
    let after_done = polls();
    sleep(Duration::from_millis(600));
    assert!(polls() <= after_done + 1, "as consultas continuaram depois de 100%");
}

#[test]
fn refuses_imu_calibration_in_flight() {
    let rig = Rig::connect();
    rig.drone.action(&Action::Takeoff).unwrap();
    rig.wait_flying(true);
    assert!(matches!(rig.drone.action(&Action::CalibrateImu), Err(Error::Refused(_))));
    assert!(!rig.sim.received().iter().any(|r| r == "app:calibration_start"));
    rig.drone.action(&Action::Land).unwrap();
}

#[test]
fn experimental_cog_calibration_only_in_a_still_low_hover() {
    let rig = Rig::connect();
    assert!(rig.drone.caps().cog_calibration);
    assert!(matches!(rig.drone.action(&Action::CalibrateCog), Err(Error::Refused(_))), "no chão");
    rig.drone.action(&Action::Takeoff).unwrap();
    rig.wait_flying(true);
    rig.drone.set_sticks(Sticks { roll: 0.5, ..Sticks::NEUTRAL });
    assert!(matches!(rig.drone.action(&Action::CalibrateCog), Err(Error::Refused(_))), "sticks fora do centro");
    assert!(!rig.sim.received().iter().any(|r| r == "app:calibrate"), "recusado antes de enviar");
    rig.drone.set_sticks(Sticks::NEUTRAL);
    assert_eq!(rig.drone.action(&Action::CalibrateCog).unwrap(), "ok");
    assert!(rig.sim.received().iter().any(|r| r == "app:calibrate"));
    let log = rig.drone.blackbox().snapshot();
    let noted = log.iter().filter(|e| e.text.contains("centro de massa (experimental)")).count();
    assert!(noted >= 3, "envio, resposta e resultado na caixa-preta ({noted})");
    rig.drone.action(&Action::Land).unwrap();
}
