//! Núcleo do cliente Tello.
//!
//! Implementa do zero os dois protocolos do drone:
//! - [`Protocol::Sdk`]: o SDK oficial em texto (UDP 8889/8890/11111);
//! - [`Protocol::App`]: o protocolo binário usado pelo app oficial.
//!
//! O ponto de entrada é [`Drone::connect`], que devolve um handle para enviar
//! ações e sticks; tudo que chega do drone (telemetria, vídeo, avisos) é
//! entregue pelo [`EventSink`].

pub mod app;
pub mod blackbox;
mod client;
pub mod config;
mod control;
pub mod error;
pub mod model;
pub mod net;
pub mod sdk;
pub mod util;
pub mod video;

pub use blackbox::{Blackbox, Channel, Dir, Entry};
pub use client::Drone;
pub use config::{Config, Safety};
pub use error::Error;
pub use model::*;
pub use video::{NalAssembler, VideoFrame};
