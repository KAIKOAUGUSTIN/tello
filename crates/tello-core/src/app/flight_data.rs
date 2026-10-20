//! Mensagens de telemetria do protocolo do app.

use super::packet::le16;
use crate::Telemetry;

/// Mensagem de voo (0x56), ~10 Hz. Layout conforme a engenharia reversa da
/// comunidade; campos são validados pelo tamanho antes de serem lidos.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlightData {
    /// Altura em decímetros.
    pub height_dm: i16,
    pub north_speed: i16,
    pub east_speed: i16,
    pub ground_speed: i16,
    /// Tempo de voo em décimos de segundo.
    pub fly_time: i16,
    pub battery: u8,
    pub em_sky: bool,
    pub em_ground: bool,
    pub battery_low: bool,
    pub battery_lower: bool,
    pub wind_state: bool,
    pub fly_mode: u8,
    pub throw_fly_timer: u8,
    pub camera_state: u8,
    pub temperature_high: bool,
}

impl FlightData {
    pub fn parse(p: &[u8]) -> Option<FlightData> {
        if p.len() < 24 {
            return None;
        }
        Some(FlightData {
            height_dm: le16(p, 0)?,
            north_speed: le16(p, 2)?,
            east_speed: le16(p, 4)?,
            ground_speed: le16(p, 6)?,
            fly_time: le16(p, 8)?,
            wind_state: p[10] >> 7 & 1 == 1,
            battery: p[12].min(100),
            em_sky: p[17] & 1 == 1,
            em_ground: p[17] >> 1 & 1 == 1,
            battery_low: p[17] >> 5 & 1 == 1,
            battery_lower: p[17] >> 6 & 1 == 1,
            fly_mode: p[18],
            throw_fly_timer: p[19],
            camera_state: p[20],
            temperature_high: p[23] & 1 == 1,
        })
    }

    /// Codifica no mesmo layout (simulador e testes).
    pub fn encode(&self) -> Vec<u8> {
        let mut p = vec![0u8; 24];
        p[0..2].copy_from_slice(&self.height_dm.to_le_bytes());
        p[2..4].copy_from_slice(&self.north_speed.to_le_bytes());
        p[4..6].copy_from_slice(&self.east_speed.to_le_bytes());
        p[6..8].copy_from_slice(&self.ground_speed.to_le_bytes());
        p[8..10].copy_from_slice(&self.fly_time.to_le_bytes());
        p[10] = (self.wind_state as u8) << 7;
        p[12] = self.battery;
        p[17] = self.em_sky as u8
            | (self.em_ground as u8) << 1
            | (self.battery_low as u8) << 5
            | (self.battery_lower as u8) << 6;
        p[18] = self.fly_mode;
        p[19] = self.throw_fly_timer;
        p[20] = self.camera_state;
        p[23] = self.temperature_high as u8;
        p
    }

    pub fn apply(&self, t: &mut Telemetry) {
        t.height_cm = Some(self.height_dm as i32 * 10);
        t.speed_x = Some(self.north_speed as i32);
        t.speed_y = Some(self.east_speed as i32);
        t.speed_z = Some(self.ground_speed as i32);
        t.flight_time_s = Some((self.fly_time.max(0) / 10) as u32);
        t.battery = Some(self.battery);
        t.battery_low = Some(self.battery_low || self.battery_lower);
        t.flying = Some(self.em_sky);
    }
}

/// Mensagem de Wi-Fi (0x1A): intensidade do sinal.
pub fn parse_wifi(p: &[u8]) -> Option<u8> {
    p.first().copied()
}

/// Mensagem de luz (0x35): luminosidade do ambiente.
pub fn parse_light(p: &[u8]) -> Option<u8> {
    p.first().copied()
}

/// Versão do firmware (0x45): texto ASCII após o primeiro byte.
pub fn parse_version(p: &[u8]) -> Option<String> {
    let text: String =
        p.get(1..)?.iter().take_while(|&&b| b != 0).filter(|b| b.is_ascii_graphic()).map(|&b| b as char).collect();
    (!text.is_empty()).then_some(text)
}

/// Limite de altura (0x1056): metros em LE16 após o primeiro byte.
pub fn parse_alt_limit(p: &[u8]) -> Option<u16> {
    le16(p, 1).map(|v| v as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_apply() {
        let fd = FlightData {
            height_dm: 15,
            north_speed: 3,
            east_speed: -2,
            fly_time: 125,
            battery: 76,
            em_sky: true,
            battery_low: true,
            ..Default::default()
        };
        let parsed = FlightData::parse(&fd.encode()).unwrap();
        assert_eq!(parsed, fd);
        let mut t = Telemetry::default();
        parsed.apply(&mut t);
        assert_eq!(t.height_cm, Some(150));
        assert_eq!(t.flight_time_s, Some(12));
        assert_eq!(t.battery, Some(76));
        assert_eq!(t.flying, Some(true));
        assert_eq!(t.battery_low, Some(true));
    }

    #[test]
    fn rejects_short_payload() {
        assert_eq!(FlightData::parse(&[0; 10]), None);
    }

    #[test]
    fn parses_small_messages() {
        assert_eq!(parse_version(b"\x0001.04.92.01\x00\x00"), Some("01.04.92.01".into()));
        assert_eq!(parse_alt_limit(&[0, 10, 0]), Some(10));
        assert_eq!(parse_wifi(&[90, 0]), Some(90));
    }
}
