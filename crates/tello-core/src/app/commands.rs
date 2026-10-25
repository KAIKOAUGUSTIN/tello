//! Tradução de [`Action`] para pacotes do protocolo do app.

use super::packet::{id, kind, Packet};
use crate::{Action, Condition, Error, FlipDir, SmartVideo, Sticks, Telemetry};

/// Altura máxima para a calibração do centro de massa: perto do chão um
/// comportamento estranho do drone faz menos estrago.
pub const COG_MAX_HEIGHT_CM: i32 = 150;

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
    /// Calibração do centro de massa (experimental), depois das checagens.
    CalibrateCog(Packet),
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
        Action::CalibrateCog => CalibrateCog(Packet::new(kind::SET, id::CALIBRATE, vec![1])),
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

/// Travas da calibração do centro de massa: pairando, baixo, sem vento e sem
/// ninguém mexendo nos sticks.
pub fn check_cog_calibration(t: &Telemetry, flying: bool, sticks: Sticks, active: &[Condition]) -> Result<(), Error> {
    let refuse = |why: &str| Err(Error::Refused(why.into()));
    if !flying {
        return refuse("decole e deixe o drone pairando antes de calibrar o centro de massa");
    }
    match t.height_cm {
        None => return refuse("a altura do drone ainda não chegou"),
        Some(h) if h > COG_MAX_HEIGHT_CM => return refuse("desça para menos de 1,5 m antes de calibrar"),
        _ => {}
    }
    if !sticks.is_neutral() {
        return refuse("solte os sticks: o drone precisa estar pairando parado");
    }
    if active.contains(&Condition::Wind) {
        return refuse("com vento a calibração sai errada: voe num lugar fechado");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cog_calibration_needs_a_low_still_hover() {
        let low = Telemetry { height_cm: Some(80), ..Default::default() };
        let still = Sticks::NEUTRAL;
        assert!(check_cog_calibration(&low, true, still, &[]).is_ok());
        assert!(check_cog_calibration(&low, true, still, &[Condition::LowLight]).is_ok());
        let refused = |r: Result<(), Error>| matches!(r, Err(Error::Refused(_)));
        assert!(refused(check_cog_calibration(&low, false, still, &[])), "no chão");
        let high = Telemetry { height_cm: Some(151), ..Default::default() };
        assert!(refused(check_cog_calibration(&high, true, still, &[])), "alto demais");
        assert!(refused(check_cog_calibration(&Telemetry::default(), true, still, &[])), "sem altura");
        let moving = Sticks { roll: 0.2, ..Sticks::NEUTRAL };
        assert!(refused(check_cog_calibration(&low, true, moving, &[])), "sticks fora do centro");
        assert!(refused(check_cog_calibration(&low, true, still, &[Condition::Wind])), "vento");
    }

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
