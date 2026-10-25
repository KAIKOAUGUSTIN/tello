//! Tradução de [`Action`] para pacotes do protocolo do app.

use super::packet::{id, kind, Packet};
use crate::{Action, Error, FlipDir, SmartVideo};

/// O que o driver deve fazer para executar uma ação.
#[derive(Clone, Debug, PartialEq)]
pub enum AppCommand {
    /// Envia o pacote e espera a confirmação do drone.
    Send(Packet),
    /// Corta os motores (texto "emergency", aceito também neste modo).
    Emergency,
    /// Sticks no centro: o drone paira.
    Hover,
    SetFast(bool),
    VideoOn,
    VideoOff,
    /// Começa a calibração da IMU e acompanha o andamento.
    CalibrateImu,
}

pub fn flip_code(dir: FlipDir) -> u8 {
    match dir {
        FlipDir::Forward => 0,
        FlipDir::Left => 1,
        FlipDir::Back => 2,
        FlipDir::Right => 3,
        FlipDir::ForwardLeft => 4,
        FlipDir::BackLeft => 5,
        FlipDir::BackRight => 6,
        FlipDir::ForwardRight => 7,
    }
}

pub fn smart_video_code(mode: SmartVideo) -> u8 {
    let m = match mode {
        SmartVideo::Spin360 => 1,
        SmartVideo::Circle => 2,
        SmartVideo::UpAndAway => 3,
    };
    (m << 2) | 1
}

pub fn translate(action: &Action) -> Result<AppCommand, Error> {
    use AppCommand::*;
    let set = |cmd: u16, payload: Vec<u8>| Send(Packet::new(kind::SET, cmd, payload));
    Ok(match action {
        Action::Takeoff => set(id::TAKEOFF, vec![]),
        Action::Land => set(id::LAND, vec![0]),
        Action::Emergency => Emergency,
        Action::Stop => Hover,
        Action::Flip { dir } => Send(Packet::new(kind::FLIP, id::FLIP, vec![flip_code(*dir)])),
        Action::ThrowAndGo => Send(Packet::new(kind::GET, id::THROW_AND_GO, vec![0])),
        Action::PalmLand => set(id::PALM_LAND, vec![0]),
        Action::Bounce { on } => set(id::BOUNCE, vec![if *on { 0x30 } else { 0x31 }]),
        Action::SmartVideo { mode } => set(id::SMART_VIDEO, vec![smart_video_code(*mode)]),
        Action::TakePhoto => set(id::TAKE_PICTURE, vec![]),
        Action::StreamOn => VideoOn,
        Action::StreamOff => VideoOff,
        Action::SetFastMode { on } => SetFast(*on),
        Action::CalibrateImu => CalibrateImu,
        Action::SetMaxHeight { m } => {
            if !(1..=30).contains(m) {
                return Err(Error::Protocol(format!("altura máxima fora do intervalo 1..30 m: {m}")));
            }
            set(id::SET_ALT_LIMIT, m.to_le_bytes().to_vec())
        }
        Action::SetVideoBitrate { level } => {
            if *level > 5 {
                return Err(Error::Protocol(format!("taxa de vídeo fora do intervalo 0..5: {level}")));
            }
            set(id::VIDEO_RATE, vec![*level])
        }
        Action::Move { .. } | Action::Rotate { .. } | Action::SetSpeed { .. } => {
            return Err(Error::NotSupported("movimentos por distância e velocidade fixa: use o modo SDK"))
        }
        Action::Raw { .. } => return Err(Error::NotSupported("comandos de texto: use o modo SDK")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_app_features() {
        let pkt = |a: Action| match translate(&a).unwrap() {
            AppCommand::Send(p) => p,
            other => panic!("esperava pacote, veio {other:?}"),
        };
        assert_eq!(pkt(Action::Takeoff).cmd, id::TAKEOFF);
        let flip = pkt(Action::Flip { dir: FlipDir::BackRight });
        assert_eq!((flip.kind, flip.payload.as_slice()), (kind::FLIP, &[6u8][..]));
        assert_eq!(pkt(Action::ThrowAndGo).kind, kind::GET);
        assert_eq!(pkt(Action::Bounce { on: true }).payload, [0x30]);
        assert_eq!(pkt(Action::SmartVideo { mode: SmartVideo::Circle }).payload, [0x09]);
        assert_eq!(pkt(Action::SetMaxHeight { m: 10 }).payload, [10, 0]);
        assert_eq!(translate(&Action::Emergency).unwrap(), AppCommand::Emergency);
        assert!(matches!(translate(&Action::Raw { text: "x".into() }), Err(Error::NotSupported(_))));
        assert!(translate(&Action::SetMaxHeight { m: 0 }).is_err());
    }
}
