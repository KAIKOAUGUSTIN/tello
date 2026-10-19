//! Testes de integração: cliente (tello-core) contra o simulador, via UDP real.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use tello_core::{Action, Config, Drone, DroneEvent, Error, FlipDir, Model, Protocol, Safety, Sticks, WarningCode};
use tello_sim::{Sim, SimConfig};

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

fn free_port() -> u16 {
    UdpSocket::bind((LOCALHOST, 0)).unwrap().local_addr().unwrap().port()
}

struct Rig {
    sim: Sim,
    config: Config,
    events: Arc<Mutex<Vec<DroneEvent>>>,
}

impl Rig {
    fn new(sim_cfg: impl FnOnce(&mut SimConfig)) -> Rig {
        let (state_port, video_port) = (free_port(), free_port());
        let mut cfg = SimConfig {
            bind: SocketAddr::new(LOCALHOST, 0),
            state_port,
            video_port,
            time_scale: 0.05,
            ..Default::default()
        };
        sim_cfg(&mut cfg);
        let sim = Sim::start(cfg).unwrap();
        let config = Config {
            drone_ip: LOCALHOST,
            command_port: sim.addr().port(),
            state_port,
            video_port,
            ..Default::default()
        };
        Rig { sim, config, events: Arc::default() }
    }

    fn connect(&self) -> Result<Drone, Error> {
        let events = self.events.clone();
        Drone::connect(self.config.clone(), Protocol::Sdk, Arc::new(move |e| events.lock().unwrap().push(e)))
    }

    fn wait_for(&self, what: &str, timeout: Duration, pred: impl Fn(&DroneEvent) -> bool) -> DroneEvent {
        let start = Instant::now();
        loop {
            if let Some(e) = self.events.lock().unwrap().iter().find(|e| pred(e)) {
                return e.clone();
            }
            assert!(start.elapsed() < timeout, "não chegou: {what}");
            sleep(Duration::from_millis(20));
        }
    }
}

fn wait_until(what: &str, timeout: Duration, cond: impl Fn() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < timeout, "condição não atingida: {what}");
        sleep(Duration::from_millis(20));
    }
}

#[test]
fn connects_and_detects_edu() {
    let rig = Rig::new(|_| {});
    let drone = rig.connect().unwrap();
    let caps = drone.caps();
    assert_eq!(caps.model, Model::TelloEdu);
    assert_eq!(caps.sdk_version.as_deref(), Some("2.0"));
    assert_eq!(caps.serial.as_deref(), Some("0TQZSIM0000001"));
    assert!(caps.stop && caps.mission_pads && !caps.throw_and_go);
    let received = rig.sim.received();
    assert_eq!(&received[..5], ["command", "sdk?", "sn?", "battery?", "wifi?"]);
    rig.wait_for("Connected", Duration::from_secs(1), |e| matches!(e, DroneEvent::Connected { .. }));
    rig.wait_for(
        "telemetria com bateria",
        Duration::from_secs(2),
        |e| matches!(e, DroneEvent::Telemetry { data } if data.battery == Some(87)),
    );
}

#[test]
fn detects_sdk_13_tello() {
    let rig = Rig::new(|c| c.sdk_version = None);
    let drone = rig.connect().unwrap();
    let caps = drone.caps();
    assert_eq!(caps.model, Model::Tello);
    assert_eq!(caps.sdk_version.as_deref(), Some("1.3"));
    assert!(!caps.stop && !caps.mission_pads);
}

#[test]
fn takeoff_sticks_and_land() {
    let rig = Rig::new(|_| {});
    let drone = rig.connect().unwrap();
    assert_eq!(drone.action(&Action::Takeoff).unwrap(), "ok");
    assert!(rig.sim.flying() && drone.is_flying());

    // Modo iniciante: 100% no stick vira 40 no rc.
    drone.set_sticks(Sticks { throttle: 1.0, yaw: -0.5, ..Sticks::NEUTRAL });
    wait_until("rc com sticks", Duration::from_secs(1), || rig.sim.last_rc() == [0, 0, 40, -20]);

    drone.set_safety(Safety::expert());
    drone.set_sticks(Sticks { pitch: 1.0, ..Sticks::NEUTRAL });
    wait_until("rc sem limite", Duration::from_secs(1), || rig.sim.last_rc() == [0, 100, 0, 0]);

    rig.wait_for(
        "altura na telemetria",
        Duration::from_secs(2),
        |e| matches!(e, DroneEvent::Telemetry { data } if data.height_cm.unwrap_or(0) >= 80),
    );
    assert_eq!(drone.action(&Action::Land).unwrap(), "ok");
    assert!(!rig.sim.flying() && !drone.is_flying());
}

#[test]
fn sticks_return_to_neutral_without_updates() {
    let rig = Rig::new(|_| {});
    let drone = rig.connect().unwrap();
    drone.set_sticks(Sticks { roll: 1.0, ..Sticks::NEUTRAL });
    wait_until("rc com roll", Duration::from_secs(1), || rig.sim.last_rc()[0] == 40);
    wait_until("rc neutro após timeout", Duration::from_secs(2), || rig.sim.last_rc() == [0; 4]);
}

#[test]
fn rc_keepalive_prevents_auto_land() {
    let rig = Rig::new(|c| c.auto_land_after = Duration::from_millis(800));
    let drone = rig.connect().unwrap();
    drone.action(&Action::Takeoff).unwrap();
    sleep(Duration::from_millis(2000));
    assert!(rig.sim.flying() && !rig.sim.auto_landed(), "o rc deve manter o drone no ar");
    drone.disconnect();
    wait_until("pouso automático do drone", Duration::from_secs(3), || rig.sim.auto_landed());
}

#[test]
fn refused_and_unsupported_commands() {
    let rig = Rig::new(|_| {});
    let drone = rig.connect().unwrap();
    assert!(matches!(drone.action(&Action::Flip { dir: FlipDir::Left }), Err(Error::Refused(_))));
    assert!(matches!(drone.action(&Action::ThrowAndGo), Err(Error::NotSupported(_))));
    assert!(matches!(drone.action(&Action::Flip { dir: FlipDir::BackLeft }), Err(Error::NotSupported(_))));
    assert_eq!(drone.action(&Action::Raw { text: "battery?".into() }).unwrap(), "87");
}

#[test]
fn streams_video_frames() {
    let rig = Rig::new(|_| {});
    let drone = rig.connect().unwrap();
    drone.action(&Action::StreamOn).unwrap();
    wait_until("20 frames de vídeo", Duration::from_secs(3), || {
        rig.events.lock().unwrap().iter().filter(|e| matches!(e, DroneEvent::Video(_))).count() >= 20
    });
    let events = rig.events.lock().unwrap();
    let frames: Vec<_> =
        events.iter().filter_map(|e| if let DroneEvent::Video(f) = e { Some(f) } else { None }).collect();
    assert!(frames[0].keyframe && frames[0].has_sps, "o primeiro frame entregue deve ser chave");
    assert!(frames.windows(2).all(|w| w[1].seq == w[0].seq + 1));
}

#[test]
fn auto_lands_on_low_battery() {
    let rig = Rig::new(|_| {});
    let drone = rig.connect().unwrap();
    drone.action(&Action::Takeoff).unwrap();
    rig.sim.set_battery(9);
    rig.wait_for("aviso de pouso automático", Duration::from_secs(2), |e| {
        matches!(e, DroneEvent::Warning { code: WarningCode::AutoLand, .. })
    });
    wait_until("pouso", Duration::from_secs(3), || !rig.sim.flying());
    assert!(rig.sim.received().iter().any(|c| c == "land"));
}

#[test]
fn warns_when_state_is_blocked() {
    // O simulador manda o estado para outra porta: é o que acontece com o firewall bloqueando.
    let rig = Rig::new(|c| c.state_port = free_port());
    let _drone = rig.connect().unwrap();
    rig.wait_for("aviso de firewall", Duration::from_secs(6), |e| {
        matches!(e, DroneEvent::Warning { code: WarningCode::NoState, .. })
    });
}

#[test]
fn times_out_without_drone() {
    let config = Config {
        drone_ip: LOCALHOST,
        command_port: free_port(),
        state_port: free_port(),
        video_port: free_port(),
        ..Default::default()
    };
    let start = Instant::now();
    let result = Drone::connect(config, Protocol::Sdk, Arc::new(|_| {}));
    assert!(matches!(result, Err(Error::Timeout(_))), "esperava timeout");
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn fixture_has_tello_like_sps() {
    use tello_core::video::{find_sps, parse_sps};
    let frames = tello_sim::fixture_frames();
    assert_eq!(frames.len(), 90);
    let sps = find_sps(&frames[0]).and_then(parse_sps).expect("SPS no primeiro frame");
    assert_eq!((sps.width, sps.height), (960, 720));
    assert_eq!((sps.profile_idc, sps.level_idc), (77, 40));
    assert_eq!(sps.codec_string(), "avc1.4D4028");
}
