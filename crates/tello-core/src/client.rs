use std::sync::Arc;

use crate::blackbox::{Blackbox, Channel};
use crate::control::Shared;
use crate::{
    app, sdk, Action, Capabilities, Config, DroneEvent, Error, EventSink, Protocol, Safety, Sticks, Telemetry,
};

/// Interface comum aos drivers de cada protocolo.
pub(crate) trait Driver: Send + Sync {
    fn caps(&self) -> Capabilities;
    fn action(&self, action: &Action) -> Result<String, Error>;
    fn shutdown(&self);
}

/// Conexão ativa com um drone.
pub struct Drone {
    shared: Arc<Shared>,
    driver: Box<dyn Driver>,
}

impl Drone {
    /// Conecta e bloqueia até o handshake terminar (alguns segundos no máximo).
    pub fn connect(config: Config, protocol: Protocol, sink: EventSink) -> Result<Drone, Error> {
        Self::connect_with_blackbox(config, protocol, sink, Arc::new(Blackbox::new(100_000)))
    }

    /// Como [`Drone::connect`], usando uma caixa-preta existente (que sobrevive entre conexões).
    pub fn connect_with_blackbox(
        config: Config,
        protocol: Protocol,
        sink: EventSink,
        blackbox: Arc<Blackbox>,
    ) -> Result<Drone, Error> {
        blackbox.info(Channel::Core, format!("conectando em {} (modo {protocol:?})", config.drone_ip));
        let shared = Arc::new(Shared::new(sink, blackbox, config.safety.clone()));
        let driver: Box<dyn Driver> = match protocol {
            Protocol::Sdk => Box::new(sdk::driver::connect(&config, shared.clone())?),
            Protocol::App => Box::new(app::driver::connect(&config, shared.clone())?),
        };
        let caps = driver.caps();
        shared.blackbox.info(Channel::Core, format!("conectado: {caps:?}"));
        shared.emit(DroneEvent::Connected { caps });
        Ok(Drone { shared, driver })
    }

    pub fn caps(&self) -> Capabilities {
        self.driver.caps()
    }

    pub fn protocol(&self) -> Protocol {
        self.driver.caps().protocol
    }

    /// Executa uma ação. Bloqueia até o drone responder (manobras podem levar segundos).
    pub fn action(&self, action: &Action) -> Result<String, Error> {
        if !self.shared.running() {
            return Err(Error::NotConnected);
        }
        self.driver.action(action)
    }

    /// Atualiza os sticks. Deve ser chamado continuamente (ex.: 30 Hz) enquanto há
    /// entrada; sem atualização por `Safety::stick_timeout_ms` eles voltam ao centro.
    pub fn set_sticks(&self, sticks: Sticks) {
        self.shared.set_sticks(sticks);
    }

    pub fn safety(&self) -> Safety {
        self.shared.safety()
    }

    pub fn set_safety(&self, safety: Safety) {
        self.shared.blackbox.info(Channel::Core, format!("segurança: {safety:?}"));
        self.shared.set_safety(safety);
    }

    pub fn telemetry(&self) -> Telemetry {
        self.shared.telemetry()
    }

    pub fn is_flying(&self) -> bool {
        self.shared.is_flying()
    }

    pub fn blackbox(&self) -> &Arc<Blackbox> {
        &self.shared.blackbox
    }

    pub fn disconnect(&self) {
        if self.shared.running.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.driver.shutdown();
            self.shared.blackbox.info(Channel::Core, "desconectado");
            self.shared.emit(DroneEvent::Disconnected { reason: "desconectado pelo usuário".into() });
        }
    }
}

impl Drop for Drone {
    fn drop(&mut self) {
        self.disconnect();
    }
}
