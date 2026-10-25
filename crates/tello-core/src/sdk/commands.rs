//! Tradução de [`Action`] para os comandos de texto do SDK.

use std::time::Duration;

use crate::{Action, Capabilities, Error, FlipDir, Model, MoveDir, Protocol, Sticks};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Leitura (`battery?`): pode repetir sem risco.
    Query,
    /// Configuração (`speed 50`, `streamon`).
    Setting,
    /// Manobra: pausa o envio de `rc` até a resposta.
    Flight,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SdkCommand {
    pub text: String,
    pub kind: Kind,
    pub timeout: Duration,
    pub retries: u8,
    /// Enviado sem esperar resposta (o drone não responde ao `rc`).
    pub fire_and_forget: bool,
}

impl SdkCommand {
    fn new(text: impl Into<String>, kind: Kind) -> Self {
        let (timeout, retries) = match kind {
            Kind::Query => (Duration::from_millis(1500), 2),
            Kind::Setting => (Duration::from_secs(5), 1),
            Kind::Flight => (Duration::from_secs(20), 0),
        };
        SdkCommand { text: text.into(), kind, timeout, retries, fire_and_forget: false }
    }

    pub fn query(text: &str) -> Self {
        Self::new(text, Kind::Query)
    }
}

const APP_ONLY: &str = "recurso do app oficial: conecte no modo App";

pub fn translate(action: &Action, caps: &Capabilities) -> Result<SdkCommand, Error> {
    use Kind::*;
    Ok(match action {
        Action::Takeoff => SdkCommand::new("takeoff", Flight),
        Action::Land => SdkCommand::new("land", Flight),
        Action::Emergency => SdkCommand { fire_and_forget: true, ..SdkCommand::new("emergency", Setting) },
        Action::Stop if caps.stop => SdkCommand::new("stop", Setting),
        // SDK 1.3 não tem `stop`: sticks no centro fazem o drone pairar.
        Action::Stop => SdkCommand { fire_and_forget: true, ..SdkCommand::new(rc(Sticks::NEUTRAL), Setting) },
        Action::Flip { dir } => {
            let letter = match dir {
                FlipDir::Forward => 'f',
                FlipDir::Back => 'b',
                FlipDir::Left => 'l',
                FlipDir::Right => 'r',
                _ => return Err(Error::NotSupported("flips diagonais: conecte no modo App")),
            };
            SdkCommand { timeout: Duration::from_secs(10), ..SdkCommand::new(format!("flip {letter}"), Flight) }
        }
        Action::ThrowAndGo
        | Action::PalmLand
        | Action::Bounce { .. }
        | Action::SmartVideo { .. }
        | Action::TakePhoto
        | Action::SetFastMode { .. }
        | Action::CalibrateImu
        | Action::CalibrateCog => return Err(Error::NotSupported(APP_ONLY)),
        Action::SetMaxHeight { .. } => {
            return Err(Error::NotSupported("no SDK a altura máxima é aplicada pelo próprio cliente"))
        }
        Action::StreamOn => SdkCommand::new("streamon", Setting),
        Action::StreamOff => SdkCommand::new("streamoff", Setting),
        Action::Move { dir, cm } => {
            check_range("distância (cm)", *cm, 20, 500)?;
            let word = match dir {
                MoveDir::Up => "up",
                MoveDir::Down => "down",
                MoveDir::Left => "left",
                MoveDir::Right => "right",
                MoveDir::Forward => "forward",
                MoveDir::Back => "back",
            };
            SdkCommand::new(format!("{word} {cm}"), Flight)
        }
        Action::Rotate { clockwise, deg } => {
            check_range("ângulo (graus)", *deg, 1, 360)?;
            SdkCommand::new(format!("{} {deg}", if *clockwise { "cw" } else { "ccw" }), Flight)
        }
        Action::SetSpeed { cm_s } => {
            check_range("velocidade (cm/s)", *cm_s, 10, 100)?;
            SdkCommand::new(format!("speed {cm_s}"), Setting)
        }
        Action::SetVideoBitrate { level } => {
            if !caps.video_bitrate {
                return Err(Error::NotSupported("taxa de vídeo exige SDK 3.0 ou o modo App"));
            }
            check_range("taxa de vídeo", *level as u16, 0, 5)?;
            SdkCommand::new(format!("setbitrate {level}"), Setting)
        }
        Action::Raw { text } => {
            let text = text.trim();
            if text.is_empty() {
                return Err(Error::Protocol("comando vazio".into()));
            }
            let first = text.split_whitespace().next().unwrap_or_default();
            let kind = if text.ends_with('?') {
                Query
            } else if is_flight_word(first) {
                Flight
            } else {
                Setting
            };
            SdkCommand::new(text, kind)
        }
    })
}

fn is_flight_word(w: &str) -> bool {
    matches!(
        w,
        "takeoff"
            | "land"
            | "flip"
            | "up"
            | "down"
            | "left"
            | "right"
            | "forward"
            | "back"
            | "cw"
            | "ccw"
            | "go"
            | "curve"
            | "jump"
    )
}

fn check_range(what: &str, v: u16, min: u16, max: u16) -> Result<(), Error> {
    if (min..=max).contains(&v) {
        Ok(())
    } else {
        Err(Error::Protocol(format!("{what} fora do intervalo {min}..{max}: {v}")))
    }
}

/// `rc a b c d`: roll, pitch, throttle, yaw em -100..100.
pub fn rc(s: Sticks) -> String {
    let v = |x: f32| (x.clamp(-1.0, 1.0) * 100.0).round() as i32;
    format!("rc {} {} {} {}", v(s.roll), v(s.pitch), v(s.throttle), v(s.yaw))
}

/// Interpreta a resposta do drone: `ok`/valor ou recusa.
pub fn classify(response: &str) -> Result<String, Error> {
    let r = response.trim().trim_end_matches('\0').trim();
    let lower = r.to_ascii_lowercase();
    if lower.starts_with("error") || lower.starts_with("unknown command") || lower.contains("out of range") {
        Err(Error::Refused(r.to_string()))
    } else {
        Ok(r.to_string())
    }
}

/// Normaliza a resposta de `sdk?` ("20" → "2.0").
pub fn parse_sdk_version(resp: &str) -> Option<String> {
    let r = resp.trim();
    if r.is_empty() || !r.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    if r.contains('.') {
        return Some(r.to_string());
    }
    let n: u32 = r.parse().ok()?;
    Some(if n >= 10 { format!("{}.{}", n / 10, n % 10) } else { format!("{n}.0") })
}

/// Capacidades a partir da versão do SDK (None = 1.3, que não conhece `sdk?`).
pub fn capabilities(sdk_version: Option<String>, serial: Option<String>) -> Capabilities {
    let major = sdk_version.as_deref().and_then(|v| v.split('.').next()?.parse::<u32>().ok()).unwrap_or(1);
    let model = match major {
        2 => Model::TelloEdu,
        3.. => Model::RoboMasterTt,
        _ => Model::Tello,
    };
    Capabilities {
        protocol: Protocol::Sdk,
        model,
        sdk_version: Some(sdk_version.unwrap_or_else(|| "1.3".into())),
        serial,
        firmware: None,
        flips: true,
        stop: major >= 2,
        mission_pads: major >= 2,
        video_bitrate: major >= 3,
        raw_commands: true,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_rc() {
        let s = Sticks { roll: -0.5, pitch: 1.0, throttle: 0.004, yaw: -1.5 };
        assert_eq!(rc(s), "rc -50 100 0 -100");
    }

    #[test]
    fn translates_actions() {
        let caps = capabilities(None, None);
        assert_eq!(translate(&Action::Takeoff, &caps).unwrap().text, "takeoff");
        assert_eq!(translate(&Action::Flip { dir: FlipDir::Left }, &caps).unwrap().text, "flip l");
        assert!(matches!(translate(&Action::Flip { dir: FlipDir::BackLeft }, &caps), Err(Error::NotSupported(_))));
        assert_eq!(translate(&Action::Move { dir: MoveDir::Forward, cm: 100 }, &caps).unwrap().text, "forward 100");
        assert!(translate(&Action::Move { dir: MoveDir::Up, cm: 10 }, &caps).is_err());
        assert_eq!(translate(&Action::Rotate { clockwise: false, deg: 90 }, &caps).unwrap().text, "ccw 90");
        assert!(matches!(translate(&Action::ThrowAndGo, &caps), Err(Error::NotSupported(_))));
        assert!(translate(&Action::Stop, &caps).unwrap().fire_and_forget, "1.3 emula stop com rc");
        let edu = capabilities(Some("2.0".into()), None);
        assert_eq!(translate(&Action::Stop, &edu).unwrap().text, "stop");
        assert_eq!(translate(&Action::Raw { text: "battery?".into() }, &caps).unwrap().kind, Kind::Query);
        assert_eq!(translate(&Action::Raw { text: "cw 30".into() }, &caps).unwrap().kind, Kind::Flight);
    }

    #[test]
    fn classifies_responses() {
        assert_eq!(classify("ok\r\n").unwrap(), "ok");
        assert_eq!(classify("87\r\n").unwrap(), "87");
        assert!(matches!(classify("error Not joystick"), Err(Error::Refused(_))));
        assert!(matches!(classify("unknown command: sdk?"), Err(Error::Refused(_))));
        assert!(matches!(classify("out of range"), Err(Error::Refused(_))));
    }

    #[test]
    fn detects_model_from_sdk() {
        assert_eq!(parse_sdk_version("20\r\n").as_deref(), Some("2.0"));
        assert_eq!(parse_sdk_version("30").as_deref(), Some("3.0"));
        assert_eq!(parse_sdk_version("unknown command: sdk?"), None);
        assert_eq!(capabilities(None, None).model, Model::Tello);
        assert_eq!(capabilities(Some("2.0".into()), None).model, Model::TelloEdu);
        let tt = capabilities(Some("3.0".into()), None);
        assert_eq!(tt.model, Model::RoboMasterTt);
        assert!(tt.video_bitrate && tt.mission_pads && tt.stop);
    }
}
