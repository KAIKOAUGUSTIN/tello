//! Registros internos que o drone envia em LOG_DATA (0x1051). Daqui sai a
//! atitude (pitch/roll/yaw) no modo App, a partir do quaternion da IMU.
//!
//! Cada registro: `0x55, len LE16, crc8, id LE16, xor, 3 bytes, payload^xor, crc16`.
//! O layout vem da engenharia reversa da comunidade, por isso o quaternion só é
//! aceito se tiver norma próxima de 1.

pub const ID_MVO: u16 = 0x001d;
pub const ID_IMU: u16 = 0x0800;
const HEADER: usize = 10;
const QUAT_OFFSET: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attitude {
    pub pitch_deg: f32,
    pub roll_deg: f32,
    pub yaw_deg: f32,
}

/// Procura um registro de IMU e devolve a atitude, se for válida.
pub fn parse_attitude(data: &[u8]) -> Option<Attitude> {
    let mut pos = 0;
    while pos + HEADER < data.len() {
        if data[pos] != 0x55 {
            pos += 1;
            continue;
        }
        let len = u16::from_le_bytes([data[pos + 1], data[pos + 2]]) as usize;
        if len < HEADER + 2 || pos + len > data.len() {
            pos += 1;
            continue;
        }
        let id = u16::from_le_bytes([data[pos + 4], data[pos + 5]]);
        if id == ID_IMU {
            let xor = data[pos + 6];
            let payload: Vec<u8> = data[pos + HEADER..pos + len - 2].iter().map(|b| b ^ xor).collect();
            if let Some(att) = quaternion_at(&payload, QUAT_OFFSET) {
                return Some(att);
            }
        }
        pos += len;
    }
    None
}

fn quaternion_at(p: &[u8], at: usize) -> Option<Attitude> {
    let f = |i: usize| -> Option<f32> { Some(f32::from_le_bytes(p.get(at + i * 4..at + i * 4 + 4)?.try_into().ok()?)) };
    let (w, x, y, z) = (f(0)?, f(1)?, f(2)?, f(3)?);
    let norm = (w * w + x * x + y * y + z * z).sqrt();
    if !(0.9..=1.1).contains(&norm) {
        return None;
    }
    let (w, x, y, z) = (w / norm, x / norm, y / norm, z / norm);
    let roll = (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y));
    let pitch = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0).asin();
    let yaw = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));
    Some(Attitude { pitch_deg: pitch.to_degrees(), roll_deg: roll.to_degrees(), yaw_deg: yaw.to_degrees() })
}

/// Monta um registro de IMU com o quaternion dado (simulador e testes).
pub fn encode_imu_record(q: [f32; 4], xor: u8) -> Vec<u8> {
    let mut payload = [0u8; QUAT_OFFSET + 16 + 48];
    for (i, v) in q.iter().enumerate() {
        payload[QUAT_OFFSET + i * 4..QUAT_OFFSET + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    let len = (HEADER + payload.len() + 2) as u16;
    let mut rec = vec![0x55];
    rec.extend_from_slice(&len.to_le_bytes());
    rec.push(0); // crc8 (não verificado pelo cliente)
    rec.extend_from_slice(&ID_IMU.to_le_bytes());
    rec.push(xor);
    rec.extend_from_slice(&[0, 0, 0]);
    rec.extend(payload.iter().map(|b| b ^ xor));
    rec.extend_from_slice(&[0, 0]);
    rec
}

/// Quaternion de uma rotação em yaw (graus), para o simulador.
pub fn yaw_quaternion(yaw_deg: f32) -> [f32; 4] {
    let h = yaw_deg.to_radians() / 2.0;
    [h.cos(), 0.0, 0.0, h.sin()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_imu_attitude() {
        let mut data = vec![0x00, 0x12]; // lixo antes do registro
        data.extend(encode_imu_record(yaw_quaternion(90.0), 0x5a));
        let att = parse_attitude(&data).unwrap();
        assert!((att.yaw_deg - 90.0).abs() < 0.1);
        assert!(att.pitch_deg.abs() < 0.1 && att.roll_deg.abs() < 0.1);
    }

    #[test]
    fn rejects_invalid_quaternion() {
        let data = encode_imu_record([5.0, 0.0, 0.0, 0.0], 0x11);
        assert_eq!(parse_attitude(&data), None);
        assert_eq!(parse_attitude(&[0x55, 0xff, 0xff, 0, 0, 8]), None);
    }
}
