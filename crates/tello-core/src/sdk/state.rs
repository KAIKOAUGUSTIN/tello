//! Parser da string de estado que o drone envia a ~10 Hz para a porta 8890.
//!
//! Exemplo (Tello comum):
//! `pitch:0;roll:0;yaw:0;vgx:0;vgy:0;vgz:0;templ:83;temph:85;tof:10;h:0;bat:91;baro:-37.40;time:0;agx:-2.00;agy:-9.00;agz:-999.00;`
//!
//! O Tello EDU acrescenta `mid:..;x:..;y:..;z:..;mpry:p,r,y;` no início.

use crate::{MissionPad, Telemetry};

/// Aplica os campos reconhecidos em `t` e devolve quantos foram lidos.
pub fn parse_state(text: &str, t: &mut Telemetry) -> usize {
    let mut parsed = 0;
    let mut pad = MissionPad { id: -1, ..Default::default() };
    let mut has_pad = false;
    let mut accel = t.accel.unwrap_or([0.0; 3]);
    let mut has_accel = false;

    for field in text.trim().split(';') {
        let Some((key, value)) = field.split_once(':') else { continue };
        let value = value.trim();
        let int = || value.parse::<f32>().ok().map(|v| v.round() as i32);
        let float = || value.parse::<f32>().ok();
        let key = key.trim();
        let ok = match key {
            "pitch" => set(&mut t.pitch_deg, int()),
            "roll" => set(&mut t.roll_deg, int()),
            "yaw" => set(&mut t.yaw_deg, int()),
            "vgx" => set(&mut t.speed_x, int()),
            "vgy" => set(&mut t.speed_y, int()),
            "vgz" => set(&mut t.speed_z, int()),
            "templ" => set(&mut t.temp_low_c, int()),
            "temph" => set(&mut t.temp_high_c, int()),
            "tof" => set(&mut t.tof_cm, int()),
            "h" => set(&mut t.height_cm, int()),
            "bat" => set(&mut t.battery, int().map(|v| v.clamp(0, 100) as u8)),
            "baro" => set(&mut t.baro_m, float()),
            "time" => set(&mut t.flight_time_s, int().map(|v| v.max(0) as u32)),
            "agx" | "agy" | "agz" => match float() {
                Some(v) => {
                    accel[usize::from(key.as_bytes()[2] - b'x')] = v;
                    has_accel = true;
                    true
                }
                None => false,
            },
            "mid" | "x" | "y" | "z" => match int() {
                Some(v) => {
                    match key {
                        "mid" => {
                            pad.id = v;
                            has_pad = true;
                        }
                        "x" => pad.x = v,
                        "y" => pad.y = v,
                        _ => pad.z = v,
                    }
                    true
                }
                None => false,
            },
            _ => false,
        };
        if ok {
            parsed += 1;
        }
    }
    if has_accel {
        t.accel = Some(accel);
    }
    if has_pad {
        // -1 / -2: nenhum mission pad detectado.
        t.mission_pad = (pad.id >= 0).then_some(pad);
    }
    parsed
}

fn set<T>(slot: &mut Option<T>, value: Option<T>) -> bool {
    match value {
        Some(v) => {
            *slot = Some(v);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tello_state() {
        let mut t = Telemetry::default();
        let n = parse_state(
            "pitch:1;roll:-2;yaw:45;vgx:3;vgy:0;vgz:-1;templ:83;temph:85;tof:10;h:120;bat:91;baro:-37.40;time:12;agx:-2.00;agy:-9.00;agz:-999.00;\r\n",
            &mut t,
        );
        assert_eq!(n, 16);
        assert_eq!(t.pitch_deg, Some(1));
        assert_eq!(t.roll_deg, Some(-2));
        assert_eq!(t.yaw_deg, Some(45));
        assert_eq!(t.speed_x, Some(3));
        assert_eq!(t.speed_z, Some(-1));
        assert_eq!(t.temp_low_c, Some(83));
        assert_eq!(t.tof_cm, Some(10));
        assert_eq!(t.height_cm, Some(120));
        assert_eq!(t.battery, Some(91));
        assert_eq!(t.baro_m, Some(-37.4));
        assert_eq!(t.flight_time_s, Some(12));
        assert_eq!(t.accel, Some([-2.0, -9.0, -999.0]));
        assert_eq!(t.mission_pad, None);
    }

    #[test]
    fn parses_edu_mission_pad() {
        let mut t = Telemetry::default();
        parse_state("mid:3;x:10;y:-20;z:85;mpry:0,0,0;pitch:0;roll:0;yaw:0;bat:50;", &mut t);
        assert_eq!(t.mission_pad, Some(MissionPad { id: 3, x: 10, y: -20, z: 85 }));
        parse_state("mid:-1;x:0;y:0;z:0;mpry:0,0,0;bat:50;", &mut t);
        assert_eq!(t.mission_pad, None);
    }

    #[test]
    fn ignores_garbage() {
        let mut t = Telemetry::default();
        assert_eq!(parse_state("ok", &mut t), 0);
        assert_eq!(parse_state("bat:abc;h:;", &mut t), 0);
        assert_eq!(t, Telemetry::default());
    }
}
