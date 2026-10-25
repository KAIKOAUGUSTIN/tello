//! Tipos compartilhados entre os dois protocolos e a UI.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::report::FlightSummary;
use crate::video::VideoFrame;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// SDK oficial em texto.
    #[default]
    Sdk,
    /// Protocolo binário do app oficial.
    App,
}

/// Comandos de voo contínuos, cada eixo em -1.0..=1.0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sticks {
    /// Esquerda (-) / direita (+).
    pub roll: f32,
    /// Trás (-) / frente (+).
    pub pitch: f32,
    /// Descer (-) / subir (+).
    pub throttle: f32,
    /// Girar anti-horário (-) / horário (+).
    pub yaw: f32,
}

impl Sticks {
    pub const NEUTRAL: Sticks = Sticks { roll: 0.0, pitch: 0.0, throttle: 0.0, yaw: 0.0 };

    pub fn clamped(self) -> Self {
        let c = |v: f32| if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
        Sticks { roll: c(self.roll), pitch: c(self.pitch), throttle: c(self.throttle), yaw: c(self.yaw) }
    }

    pub fn scaled(self, k: f32) -> Self {
        Sticks { roll: self.roll * k, pitch: self.pitch * k, throttle: self.throttle * k, yaw: self.yaw * k }
    }

    pub fn is_neutral(&self) -> bool {
        *self == Self::NEUTRAL
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlipDir {
    Forward,
    Back,
    Left,
    Right,
    ForwardLeft,
    ForwardRight,
    BackLeft,
    BackRight,
}

impl FlipDir {
    pub fn is_diagonal(self) -> bool {
        matches!(self, FlipDir::ForwardLeft | FlipDir::ForwardRight | FlipDir::BackLeft | FlipDir::BackRight)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveDir {
    Up,
    Down,
    Left,
    Right,
    Forward,
    Back,
}

/// EZ Shots do app oficial.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartVideo {
    Spin360,
    Circle,
    UpAndAway,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Takeoff,
    Land,
    /// Corta os motores imediatamente.
    Emergency,
    /// Para e paira no lugar.
    Stop,
    Flip {
        dir: FlipDir,
    },
    ThrowAndGo,
    PalmLand,
    Bounce {
        on: bool,
    },
    SmartVideo {
        mode: SmartVideo,
    },
    TakePhoto,
    StreamOn,
    StreamOff,
    Move {
        dir: MoveDir,
        cm: u16,
    },
    Rotate {
        clockwise: bool,
        deg: u16,
    },
    SetSpeed {
        cm_s: u16,
    },
    /// Ativa o "modo rápido" dos sticks (só no protocolo do app).
    SetFastMode {
        on: bool,
    },
    SetMaxHeight {
        m: u16,
    },
    /// 0 = automático; 1..5 = 1, 1,5, 2, 3 e 4 Mbps.
    SetVideoBitrate {
        level: u8,
    },
    /// Calibração da IMU (só no protocolo do app, com o drone no chão).
    CalibrateImu,
    /// Calibração do centro de massa (experimental): só no protocolo do app,
    /// com o drone pairando baixo e os sticks no centro.
    CalibrateCog,
    /// Texto cru do SDK (apenas para depuração).
    Raw {
        text: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Model {
    #[default]
    Unknown,
    Tello,
    TelloEdu,
    RoboMasterTt,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub protocol: Protocol,
    pub model: Model,
    pub sdk_version: Option<String>,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub flips: bool,
    pub flips_diagonal: bool,
    pub throw_and_go: bool,
    pub palm_land: bool,
    pub bounce: bool,
    pub smart_video: bool,
    pub hi_res_photo: bool,
    pub fast_mode: bool,
    pub max_height: bool,
    pub video_bitrate: bool,
    pub stop: bool,
    pub mission_pads: bool,
    pub raw_commands: bool,
    pub imu_calibration: bool,
    /// Calibração do centro de massa (experimental).
    pub cog_calibration: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MissionPad {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Telemetria normalizada. Campos ausentes no protocolo atual ficam `None`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Telemetry {
    pub battery: Option<u8>,
    /// Altura relativa à decolagem, em cm.
    pub height_cm: Option<i32>,
    /// Distância ao chão pelo sensor ToF, em cm.
    pub tof_cm: Option<i32>,
    pub baro_m: Option<f32>,
    pub temp_low_c: Option<i32>,
    pub temp_high_c: Option<i32>,
    pub pitch_deg: Option<i32>,
    pub roll_deg: Option<i32>,
    pub yaw_deg: Option<i32>,
    /// Velocidades em dm/s.
    pub speed_x: Option<i32>,
    pub speed_y: Option<i32>,
    pub speed_z: Option<i32>,
    /// Acelerações em 0,001 g.
    pub accel: Option<[f32; 3]>,
    /// Tempo de motor ligado, em segundos.
    pub flight_time_s: Option<u32>,
    pub wifi_strength: Option<u8>,
    pub flying: Option<bool>,
    pub mission_pad: Option<MissionPad>,
    pub battery_low: Option<bool>,
    /// Luz para a câmera de baixo (modo App): 0 = suficiente, 1 = pouca luz.
    pub light_strength: Option<u8>,
    pub fast_mode: Option<bool>,
    /// Vento forte detectado pelo drone (modo App).
    pub wind: Option<bool>,
    /// Interferência no Wi-Fi, em % (modo App).
    pub wifi_interference: Option<u8>,
}

/// Condição do ambiente ou do link que costuma deixar o voo instável.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// Pouca luz para a câmera de baixo: o drone perde a referência e deriva.
    LowLight,
    Wind,
    WeakSignal,
    /// Alto demais para a câmera de baixo enxergar o chão.
    HighAltitude,
}

impl Condition {
    pub fn warning(self) -> WarningCode {
        match self {
            Condition::LowLight => WarningCode::LowLight,
            Condition::Wind => WarningCode::Wind,
            Condition::WeakSignal => WarningCode::WeakSignal,
            Condition::HighAltitude => WarningCode::HighAltitude,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Condition::LowLight => {
                "Pouca luz para a câmera de baixo: o drone pode derivar. Voe num lugar mais claro e sobre um chão com textura."
            }
            Condition::Wind => "O drone detectou vento forte. Voe baixo e perto, ou pouse.",
            Condition::WeakSignal => "Sinal Wi-Fi fraco. Aproxime-se do drone e evite paredes entre vocês.",
            Condition::HighAltitude => {
                "Acima de 6 m a câmera de baixo não enxerga o chão e o drone pode derivar. Desça um pouco."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    /// Os comandos funcionam, mas o estado (porta 8890) não chega: firewall.
    NoState,
    /// O vídeo foi ligado mas nenhum pacote chegou: firewall.
    NoVideo,
    /// A telemetria parou de chegar durante a conexão.
    LinkLost,
    LinkRestored,
    /// Pouso automático por bateria baixa.
    AutoLand,
    /// Condições de voo (ver [`Condition`]).
    LowLight,
    Wind,
    WeakSignal,
    HighAltitude,
    Other,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DroneEvent {
    Connected {
        caps: Capabilities,
    },
    Disconnected {
        reason: String,
    },
    /// O link caiu e o app está refazendo a conexão (sockets e handshake).
    Reconnecting {
        attempt: u32,
    },
    Telemetry {
        data: Telemetry,
    },
    Warning {
        code: WarningCode,
        message: String,
    },
    /// O conjunto de condições ativas mudou.
    Conditions {
        active: Vec<Condition>,
    },
    /// Resumo do vídeo no último segundo (enquanto o vídeo está ligado).
    VideoStats(VideoStats),
    PhotoSaved {
        path: String,
    },
    /// Andamento da calibração da IMU.
    Calibration(Calibration),
    /// O voo terminou; `path` é o HTML salvo (quando há pasta configurada).
    FlightReport {
        path: Option<String>,
        summary: FlightSummary,
    },
    /// Frame H.264 completo (Annex B). Não é serializado: vai por canal binário.
    #[serde(skip)]
    Video(VideoFrame),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct VideoStats {
    pub fps: u16,
    pub kbps: u32,
    /// Pacotes perdidos, em % (só no modo App, que numera os pacotes).
    pub loss_pct: Option<f32>,
    /// Nível de taxa em uso (1..5), quando foi o app que escolheu.
    pub level: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationState {
    Running,
    /// Terminou: o drone precisa ser desligado e ligado de novo.
    Done,
    /// O drone não chegou a 100% no tempo limite.
    TimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Calibration {
    /// Posições já aceitas pelo drone (0..=6).
    pub steps: u8,
    /// Andamento informado pelo drone, em %.
    pub progress: u8,
    pub state: CalibrationState,
}

pub type EventSink = Arc<dyn Fn(DroneEvent) + Send + Sync>;
