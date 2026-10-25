//! Protocolo do app no simulador: handshake, confirmações, telemetria binária,
//! sticks e transferência de foto.

use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tello_core::app::flight_data::FlightData;
use tello_core::app::log_data::{encode_imu_record, yaw_quaternion};
use tello_core::app::photo::{self, Announce};
use tello_core::app::{id, stick, Packet};

use crate::{Inner, State, FIXTURE_JPEG};

/// Tipo dos pacotes que saem do drone.
const FROM_DRONE: u8 = 0x88;
const FILE_ID: u16 = 1;

impl Inner {
    fn send_app(&self, to: SocketAddr, cmd: u16, payload: Vec<u8>) {
        let _ = self.sock.send_to(&Packet::new(FROM_DRONE, cmd, payload).encode(), to);
    }

    fn note(&self, what: impl Into<String>) {
        let what = what.into();
        if self.verbose {
            println!("<- {what}");
        }
        self.received.lock().unwrap().push(what);
    }

    pub(crate) fn handle_app(self: &Arc<Self>, data: &[u8], from: SocketAddr) {
        if let Some(port) = data.strip_prefix(b"conn_req:") {
            let mut st = self.state.lock().unwrap();
            st.app_mode = true;
            st.client = Some(from);
            st.app_video_port = match port {
                [a, b, ..] => u16::from_le_bytes([*a, *b]),
                _ => 6038,
            };
            st.last_cmd = Instant::now();
            drop(st);
            self.note("conn_req");
            let mut ack = b"conn_ack:".to_vec();
            ack.extend_from_slice(port);
            let _ = self.sock.send_to(&ack, from);
            // O drone real manda um cabeçalho de log que precisa ser confirmado.
            self.send_app(from, id::LOG_HEADER, vec![0x34, 0x12, 0, 0]);
            return;
        }
        if data == b"emergency" {
            let mut st = self.state.lock().unwrap();
            st.flying = false;
            st.height_cm = 0.0;
            drop(st);
            self.note("emergency");
            return;
        }
        let Ok(p) = Packet::decode(data) else {
            self.note("pacote inválido");
            return;
        };
        let mut st = self.state.lock().unwrap();
        if !st.app_mode {
            return;
        }
        st.last_cmd = Instant::now();
        if p.cmd == id::STICK {
            if let Some((s, fast)) = stick::decode(&p.payload) {
                let v = |x: f32| (x * 100.0).round() as i32;
                st.rc = [v(s.roll), v(s.pitch), v(s.throttle), v(s.yaw)];
                st.fast = fast;
            }
            return;
        }
        let (flying, battery) = (st.flying, st.battery);
        drop(st);
        self.note(format!("app:{}", id::name(p.cmd)));

        let ack = |code: u8| self.send_app(from, p.cmd, vec![code]);
        match p.cmd {
            id::TAKEOFF => {
                ack(0);
                self.after(1.0, |s| {
                    s.flying = true;
                    s.height_cm = 80.0;
                });
            }
            id::THROW_AND_GO => {
                ack(0);
                self.after(1.5, |s| {
                    s.flying = true;
                    s.height_cm = 120.0;
                });
            }
            id::LAND | id::PALM_LAND => {
                ack(0);
                self.after(1.0, |s| {
                    s.flying = false;
                    s.height_cm = 0.0;
                });
            }
            id::FLIP => ack(if flying && battery >= 50.0 { 0 } else { 1 }),
            id::SMART_VIDEO => {
                ack(if flying { 0 } else { 1 });
                self.send_app(from, id::SMART_VIDEO_STATUS, vec![p.payload.first().copied().unwrap_or(0)]);
            }
            id::VIDEO_RATE => {
                self.state.lock().unwrap().video_rate = p.payload.first().copied();
                ack(0);
            }
            id::BOUNCE | id::SET_ALT_LIMIT | id::EXPOSURE | id::VIDEO_MODE => ack(0),
            // Comportamento do drone real desconhecido: aceita só pairando.
            id::CALIBRATE => ack(if flying { 0 } else { 1 }),
            id::VERSION => {
                let mut v = vec![0];
                v.extend_from_slice(b"01.04.92.01\0");
                self.send_app(from, id::VERSION, v);
            }
            id::ALT_LIMIT => self.send_app(from, id::ALT_LIMIT, vec![0, 10, 0]),
            id::CALIBRATION_START => {
                self.state.lock().unwrap().calibration = Some(Instant::now());
                self.send_app(from, id::CALIBRATION_START, vec![0]);
            }
            id::CALIBRATION_STATUS => {
                // O "usuário" leva 30 s (escalados) para passar pelas 6 posições.
                let started = self.state.lock().unwrap().calibration;
                let progress = started.map_or(0.0, |t| t.elapsed().as_secs_f32() / self.scaled(30.0).as_secs_f32());
                let progress = (progress * 100.0).min(100.0) as u8;
                let steps = (progress as u32 * 6 / 100) as u8;
                self.send_app(from, id::CALIBRATION_STATUS, vec![0, (1u8 << steps) - 1, 0, progress, 0]);
            }
            id::VIDEO_SPS_PPS => self.state.lock().unwrap().streaming = true,
            id::TAKE_PICTURE => {
                ack(0);
                self.send_photo(from);
            }
            _ => {}
        }
    }

    fn after(self: &Arc<Self>, secs: f32, effect: fn(&mut State)) {
        let inner = self.clone();
        let delay = self.scaled(secs);
        thread::spawn(move || {
            thread::sleep(delay);
            effect(&mut inner.state.lock().unwrap());
        });
    }

    fn send_photo(self: &Arc<Self>, to: SocketAddr) {
        let inner = self.clone();
        thread::spawn(move || {
            let size = FIXTURE_JPEG.len() as u32;
            inner.send_app(to, id::FILE_SIZE, photo::encode_announce(Announce { kind: 1, size, file_id: FILE_ID }));
            for f in photo::split(FILE_ID, FIXTURE_JPEG) {
                inner.send_app(to, id::FILE_DATA, photo::encode_fragment(&f));
                thread::sleep(Duration::from_micros(300));
            }
        });
    }

    /// Telemetria do modo App, chamada a cada 100 ms.
    pub(crate) fn app_telemetry(&self, s: &State, tick: u32) {
        let Some(to) = s.client else { return };
        let fd = FlightData {
            height_dm: if s.flying { (s.height_cm / 10.0) as i16 } else { 0 },
            north_speed: (s.rc[1] / 20) as i16,
            east_speed: (s.rc[0] / 20) as i16,
            ground_speed: (-s.rc[2] / 20) as i16,
            fly_time: (s.motor_time_s * 10.0) as i16,
            battery: s.battery.round() as u8,
            em_sky: s.flying,
            em_ground: !s.flying,
            battery_low: s.battery < 20.0,
            wind_state: s.wind,
            ..Default::default()
        };
        self.send_app(to, id::FLIGHT, fd.encode());
        if tick % 2 == 0 {
            let mut log = vec![0u8];
            log.extend(encode_imu_record(yaw_quaternion(s.yaw_deg), 0x5a));
            self.send_app(to, id::LOG_DATA, log);
        }
        if tick % 5 == 0 {
            self.send_app(to, id::WIFI, vec![s.wifi.0, s.wifi.1]);
            self.send_app(to, id::LIGHT, vec![s.light_poor as u8]);
        }
    }
}
