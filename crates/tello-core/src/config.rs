use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const DEFAULT_DRONE_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 10, 1));

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub drone_ip: IpAddr,
    /// Porta de comandos no drone (SDK e protocolo do app).
    pub command_port: u16,
    /// Porta local para comandos; 0 = qualquer porta livre.
    pub local_command_port: u16,
    /// Porta local onde o drone entrega o estado do SDK.
    pub state_port: u16,
    /// Porta local onde o drone entrega o vídeo no modo SDK.
    pub video_port: u16,
    /// Porta local de vídeo anunciada no handshake do protocolo do app.
    pub app_video_port: u16,
    /// Pasta para as fotos em alta resolução (protocolo do app).
    pub photo_dir: Option<PathBuf>,
    pub safety: Safety,
    pub video: VideoPolicy,
    /// Refaz a conexão sozinho quando a telemetria para de chegar.
    pub auto_reconnect: bool,
    /// Pasta para o relatório de cada voo (`voo-<data>.html`).
    pub report_dir: Option<PathBuf>,
    /// Fuso do usuário em minutos a partir do UTC, para a hora do relatório.
    pub utc_offset_min: i32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            drone_ip: DEFAULT_DRONE_IP,
            command_port: 8889,
            local_command_port: 0,
            state_port: 8890,
            video_port: 11111,
            app_video_port: 6038,
            photo_dir: None,
            safety: Safety::default(),
            video: VideoPolicy::default(),
            auto_reconnect: true,
            report_dir: None,
            utc_offset_min: 0,
        }
    }
}

/// Taxa do vídeo. Níveis do drone: 0 = automático, 1 = 1 Mbps, 2 = 1,5 Mbps,
/// 3 = 2 Mbps, 4 = 3 Mbps e 5 = 4 Mbps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoPolicy {
    /// Baixa a taxa quando o vídeo perde pacotes e sobe de novo quando o link
    /// melhora (só no modo App, que informa a perda).
    pub adaptive: bool,
    /// Com `adaptive`, o nível máximo; sem, o nível fixo (0 = o drone escolhe).
    pub level: u8,
}

impl VideoPolicy {
    /// Nível máximo do modo adaptativo (o nível 0 vira o padrão).
    pub fn max_level(&self) -> u8 {
        if self.level == 0 {
            VideoPolicy::default().level
        } else {
            self.level.min(5)
        }
    }
}

impl Default for VideoPolicy {
    fn default() -> Self {
        VideoPolicy { adaptive: true, level: 4 }
    }
}

/// Limites aplicados pelo núcleo, independentemente da fonte de entrada.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Safety {
    /// Fração máxima dos sticks (0.0..=1.0).
    pub stick_limit: f32,
    /// Altura máxima em cm; acima dela o comando de subir é ignorado.
    pub max_height_cm: Option<u16>,
    /// Bateria (%) em que o drone pousa sozinho.
    pub auto_land_battery: Option<u8>,
    /// Sem atualização dos sticks por este tempo, eles voltam ao centro.
    pub stick_timeout_ms: u32,
}

impl Safety {
    /// Modo iniciante: o padrão.
    pub fn beginner() -> Self {
        Safety { stick_limit: 0.4, max_height_cm: Some(300), auto_land_battery: Some(10), stick_timeout_ms: 500 }
    }

    pub fn expert() -> Self {
        Safety { stick_limit: 1.0, max_height_cm: None, auto_land_battery: Some(10), stick_timeout_ms: 500 }
    }
}

impl Default for Safety {
    fn default() -> Self {
        Safety::beginner()
    }
}
