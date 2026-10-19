//! Pacote de sticks (0x50): 4 eixos de 11 bits (364..1684, centro 1024) e o bit
//! de modo rápido, compactados em 6 bytes, seguidos da hora atual.

use crate::Sticks;

pub const CENTER: u16 = 1024;
pub const RANGE: f32 = 660.0;

fn axis(v: f32) -> u64 {
    let v = if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
    ((CENTER as f32 + RANGE * v).round() as u64) & 0x7ff
}

/// Payload de 11 bytes: eixos + modo rápido (6 bytes) e hora h, m, s, ms LE16.
pub fn payload(s: Sticks, fast: bool, (h, m, sec, ms): (u8, u8, u8, u16)) -> [u8; 11] {
    let packed = axis(s.roll) | axis(s.pitch) << 11 | axis(s.throttle) << 22 | axis(s.yaw) << 33 | (fast as u64) << 44;
    let bytes = packed.to_le_bytes();
    let ms = ms.to_le_bytes();
    [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], h, m, sec, ms[0], ms[1]]
}

/// Operação inversa (usada pelo simulador e nos testes). Valores em -1.0..=1.0.
pub fn decode(p: &[u8]) -> Option<(Sticks, bool)> {
    let mut b = [0u8; 8];
    b[..6].copy_from_slice(p.get(..6)?);
    let packed = u64::from_le_bytes(b);
    let get = |shift: u32| (((packed >> shift) & 0x7ff) as f32 - CENTER as f32) / RANGE;
    let s = Sticks { roll: get(0), pitch: get(11), throttle: get(22), yaw: get(33) };
    Some((s, (packed >> 44) & 1 == 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_is_centered() {
        let p = payload(Sticks::NEUTRAL, false, (12, 34, 56, 789));
        let packed = u64::from_le_bytes([p[0], p[1], p[2], p[3], p[4], p[5], 0, 0]);
        for shift in [0, 11, 22, 33] {
            assert_eq!((packed >> shift) & 0x7ff, 1024);
        }
        assert_eq!(&p[6..], &[12, 34, 56, 0x15, 0x03]);
    }

    #[test]
    fn extremes_and_roundtrip() {
        let s = Sticks { roll: 1.0, pitch: -1.0, throttle: 0.5, yaw: -0.25 };
        let p = payload(s, true, (0, 0, 0, 0));
        let packed = u64::from_le_bytes([p[0], p[1], p[2], p[3], p[4], p[5], 0, 0]);
        assert_eq!(packed & 0x7ff, 1684);
        assert_eq!((packed >> 11) & 0x7ff, 364);
        let (back, fast) = decode(&p).unwrap();
        assert!(fast);
        assert!((back.throttle - 0.5).abs() < 0.01 && (back.yaw + 0.25).abs() < 0.01);
    }
}
