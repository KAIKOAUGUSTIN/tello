//! Protocolo binário do app oficial do Tello (engenharia reversa da comunidade).
//!
//! Comandos e telemetria usam a mesma porta UDP 8889 do drone; o vídeo chega na
//! porta anunciada no handshake (`conn_req:` + porta LE16).

pub mod commands;
pub mod crc;
pub(crate) mod driver;
pub mod flight_data;
pub mod log_data;
pub mod packet;
pub mod photo;
pub mod stick;

pub use packet::{id, kind, Packet};
