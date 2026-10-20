//! SDK oficial do Tello: comandos de texto via UDP.
//!
//! - comandos: `192.168.10.1:8889` (respostas voltam para a porta de origem);
//! - estado: o drone envia para a porta local 8890 (~10 Hz);
//! - vídeo: após `streamon`, H.264 cru para a porta local 11111.

pub mod commands;
pub(crate) mod driver;
pub mod state;

pub use commands::{rc, SdkCommand};
pub use state::parse_state;
