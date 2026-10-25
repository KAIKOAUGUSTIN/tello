//! Formato dos pacotes do protocolo do app:
//!
//! ```text
//! 0     1..3        3     4     5..7    7..9   9..n-2   n-2..n
//! 0xCC  tamanho<<3  CRC8  tipo  cmd LE  seq LE payload  CRC16 LE
//! ```
//!
//! O tipo codifica a direção e a classe do pacote (0x68 = set, 0x48 = get,
//! 0x50/0x60 = dados, 0x70 = flip).

use super::crc::{crc16, crc8};

pub const START: u8 = 0xcc;
/// Bytes fora do payload: início, tamanho, CRC8, tipo, cmd, seq, CRC16.
pub const OVERHEAD: usize = 11;

pub mod kind {
    pub const SET: u8 = 0x68;
    pub const GET: u8 = 0x48;
    pub const DATA1: u8 = 0x50;
    pub const DATA2: u8 = 0x60;
    pub const FLIP: u8 = 0x70;
}

/// IDs de comandos e mensagens (engenharia reversa da comunidade).
pub mod id {
    pub const WIFI: u16 = 0x001a;
    pub const VIDEO_RATE: u16 = 0x0020;
    pub const VIDEO_SPS_PPS: u16 = 0x0025;
    pub const TAKE_PICTURE: u16 = 0x0030;
    pub const VIDEO_MODE: u16 = 0x0031;
    pub const EXPOSURE: u16 = 0x0034;
    pub const LIGHT: u16 = 0x0035;
    pub const VERSION: u16 = 0x0045;
    pub const TIME: u16 = 0x0046;
    pub const STICK: u16 = 0x0050;
    pub const TAKEOFF: u16 = 0x0054;
    pub const LAND: u16 = 0x0055;
    pub const FLIGHT: u16 = 0x0056;
    pub const SET_ALT_LIMIT: u16 = 0x0058;
    /// Começa a calibração da IMU (o usuário gira o drone pelas 6 posições).
    pub const CALIBRATION_START: u16 = 0x005a;
    /// Andamento da calibração: `[_, máscara das posições, _, progresso %, _]`.
    pub const CALIBRATION_STATUS: u16 = 0x005b;
    pub const FLIP: u16 = 0x005c;
    pub const THROW_AND_GO: u16 = 0x005d;
    pub const PALM_LAND: u16 = 0x005e;
    pub const FILE_SIZE: u16 = 0x0062;
    pub const FILE_DATA: u16 = 0x0063;
    pub const FILE_COMPLETE: u16 = 0x0064;
    pub const SMART_VIDEO: u16 = 0x0080;
    pub const SMART_VIDEO_STATUS: u16 = 0x0081;
    pub const LOG_HEADER: u16 = 0x1050;
    pub const LOG_DATA: u16 = 0x1051;
    pub const LOG_CONFIG: u16 = 0x1052;
    pub const BOUNCE: u16 = 0x1053;
    pub const ALT_LIMIT: u16 = 0x1056;

    pub fn name(id: u16) -> &'static str {
        match id {
            WIFI => "wifi",
            VIDEO_RATE => "video_rate",
            VIDEO_SPS_PPS => "video_sps_pps",
            TAKE_PICTURE => "take_picture",
            VIDEO_MODE => "video_mode",
            EXPOSURE => "exposure",
            LIGHT => "light",
            VERSION => "version",
            TIME => "time",
            STICK => "stick",
            TAKEOFF => "takeoff",
            LAND => "land",
            FLIGHT => "flight",
            SET_ALT_LIMIT => "set_alt_limit",
            CALIBRATION_START => "calibration_start",
            CALIBRATION_STATUS => "calibration_status",
            FLIP => "flip",
            THROW_AND_GO => "throw_and_go",
            PALM_LAND => "palm_land",
            FILE_SIZE => "file_size",
            FILE_DATA => "file_data",
            FILE_COMPLETE => "file_complete",
            SMART_VIDEO => "smart_video",
            SMART_VIDEO_STATUS => "smart_video_status",
            LOG_HEADER => "log_header",
            LOG_DATA => "log_data",
            LOG_CONFIG => "log_config",
            BOUNCE => "bounce",
            ALT_LIMIT => "alt_limit",
            _ => "?",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub kind: u8,
    pub cmd: u16,
    pub seq: u16,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn new(kind: u8, cmd: u16, payload: impl Into<Vec<u8>>) -> Self {
        Packet { kind, cmd, seq: 0, payload: payload.into() }
    }

    pub fn encode(&self) -> Vec<u8> {
        let len = (self.payload.len() + OVERHEAD) as u16;
        let mut buf = Vec::with_capacity(len as usize);
        buf.push(START);
        buf.extend_from_slice(&(len << 3).to_le_bytes());
        buf.push(crc8(&buf[..3]));
        buf.push(self.kind);
        buf.extend_from_slice(&self.cmd.to_le_bytes());
        buf.extend_from_slice(&self.seq.to_le_bytes());
        buf.extend_from_slice(&self.payload);
        let crc = crc16(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());
        buf
    }

    /// Decodifica e valida tamanho e CRCs.
    pub fn decode(buf: &[u8]) -> Result<Packet, DecodeError> {
        if buf.len() < OVERHEAD {
            return Err(DecodeError::TooShort);
        }
        if buf[0] != START {
            return Err(DecodeError::NotAPacket);
        }
        let len = (u16::from_le_bytes([buf[1], buf[2]]) >> 3) as usize;
        if len < OVERHEAD || len > buf.len() {
            return Err(DecodeError::BadLength);
        }
        if crc8(&buf[..3]) != buf[3] {
            return Err(DecodeError::BadCrc8);
        }
        let body = &buf[..len - 2];
        if crc16(body).to_le_bytes() != [buf[len - 2], buf[len - 1]] {
            return Err(DecodeError::BadCrc16);
        }
        Ok(Packet {
            kind: buf[4],
            cmd: u16::from_le_bytes([buf[5], buf[6]]),
            seq: u16::from_le_bytes([buf[7], buf[8]]),
            payload: buf[9..len - 2].to_vec(),
        })
    }

    pub fn describe(&self) -> String {
        format!(
            "{} (0x{:04x}) tipo 0x{:02x} seq {} [{} bytes]",
            id::name(self.cmd),
            self.cmd,
            self.kind,
            self.seq,
            self.payload.len()
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    TooShort,
    NotAPacket,
    BadLength,
    BadCrc8,
    BadCrc16,
}

/// Lê um inteiro LE de 16 bits do payload, se houver bytes suficientes.
pub fn le16(p: &[u8], at: usize) -> Option<i16> {
    Some(i16::from_le_bytes([*p.get(at)?, *p.get(at + 1)?]))
}

pub fn le_u32(p: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(p.get(at..at + 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_known_takeoff() {
        let p = Packet { kind: kind::SET, cmd: id::TAKEOFF, seq: 0x01e4, payload: vec![] };
        assert_eq!(p.encode(), [0xcc, 0x58, 0x00, 0x7c, 0x68, 0x54, 0x00, 0xe4, 0x01, 0xc2, 0x16]);
    }

    #[test]
    fn roundtrip_and_validation() {
        let p = Packet { kind: kind::DATA2, cmd: id::STICK, seq: 7, payload: (0..40).collect() };
        let mut bytes = p.encode();
        assert_eq!(Packet::decode(&bytes).unwrap(), p);
        bytes[12] ^= 0xff;
        assert_eq!(Packet::decode(&bytes), Err(DecodeError::BadCrc16));
        assert_eq!(Packet::decode(b"conn_ack:\x96\x17"), Err(DecodeError::NotAPacket));
        assert_eq!(Packet::decode(&[0xcc, 0x58]), Err(DecodeError::TooShort));
    }
}
