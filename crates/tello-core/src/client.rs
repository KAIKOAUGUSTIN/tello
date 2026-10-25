use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::blackbox::{Blackbox, Channel};
use crate::control::Shared;
use crate::report::ReportOptions;
use crate::{
    app, sdk, Action, Capabilities, Config, DroneEvent, Error, EventSink, Protocol, Safety, Sticks, Telemetry,
    VideoPolicy,
};

/// Sem telemetria por este tempo, o driver é refeito do zero (sockets novos:
/// no celular, os antigos ficam presos à rede Wi-Fi que caiu).
const RELINK_AFTER: Duration = Duration::from_secs(5);
/// Intervalo entre as tentativas de reconexão.
const RETRY_EVERY: Duration = Duration::from_secs(2);
const WATCH_TICK: Duration = Duration::from_millis(200);

/// Interface comum aos drivers de cada protocolo.
pub(crate) trait Driver: Send + Sync {
    fn caps(&self) -> Capabilities;
    fn action(&self, action: &Action) -> Result<String, Error>;
    /// Encerra as threads e os sockets deste driver (a sessão continua).
    fn shutdown(&self);
}

/// Conexão ativa com um drone.
pub struct Drone {
    session: Arc<Session>,
    relink: Mutex<Option<JoinHandle<()>>>,
}

/// Estado que sobrevive às reconexões; o driver é trocado a cada uma.
struct Session {
    shared: Arc<Shared>,
    config: Config,
    protocol: Protocol,
    /// `None` enquanto a conexão está sendo refeita.
    driver: RwLock<Option<Arc<dyn Driver>>>,
    /// Últimas conhecidas, para quando não há driver.
    caps: Mutex<Capabilities>,
}

fn open(config: &Config, protocol: Protocol, shared: &Arc<Shared>) -> Result<Arc<dyn Driver>, Error> {
    Ok(match protocol {
        Protocol::Sdk => Arc::new(sdk::driver::connect(config, shared.clone())?),
        Protocol::App => Arc::new(app::driver::connect(config, shared.clone())?),
    })
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
        shared.set_video_policy(config.video);
        shared.set_report_options(ReportOptions {
            dir: config.report_dir.clone(),
            utc_offset_min: config.utc_offset_min,
        });
        let driver = open(&config, protocol, &shared)?;
        let caps = driver.caps();
        shared.blackbox.info(Channel::Core, format!("conectado: {caps:?}"));
        shared.emit(DroneEvent::Connected { caps: caps.clone() });
        let session =
            Arc::new(Session { shared, config, protocol, driver: RwLock::new(Some(driver)), caps: Mutex::new(caps) });
        let relink = session.config.auto_reconnect.then(|| {
            let session = session.clone();
            thread::Builder::new()
                .name("tello-relink".into())
                .spawn(move || session.watch())
                .expect("falha ao criar thread")
        });
        Ok(Drone { session, relink: Mutex::new(relink) })
    }

    fn shared(&self) -> &Shared {
        &self.session.shared
    }

    /// As do driver ativo; durante a reconexão, as últimas conhecidas.
    pub fn caps(&self) -> Capabilities {
        let mut last = self.session.caps.lock().unwrap();
        if let Some(driver) = self.session.driver() {
            *last = driver.caps();
        }
        last.clone()
    }

    pub fn protocol(&self) -> Protocol {
        self.session.protocol
    }

    /// A conexão caiu e está sendo refeita.
    pub fn is_reconnecting(&self) -> bool {
        self.shared().running() && self.session.driver().is_none()
    }

    /// Executa uma ação. Bloqueia até o drone responder (manobras podem levar segundos).
    pub fn action(&self, action: &Action) -> Result<String, Error> {
        if !self.shared().running() {
            return Err(Error::NotConnected);
        }
        let driver = self.session.driver().ok_or(Error::Reconnecting)?;
        let result = driver.action(action);
        if result.is_ok() {
            match action {
                Action::StreamOn => self.shared().set_video_wanted(true),
                Action::StreamOff => self.shared().set_video_wanted(false),
                _ => {}
            }
        }
        result
    }

    /// Atualiza os sticks. Deve ser chamado continuamente (ex.: 30 Hz) enquanto há
    /// entrada; sem atualização por `Safety::stick_timeout_ms` eles voltam ao centro.
    pub fn set_sticks(&self, sticks: Sticks) {
        self.shared().set_sticks(sticks);
    }

    pub fn safety(&self) -> Safety {
        self.shared().safety()
    }

    pub fn set_safety(&self, safety: Safety) {
        self.shared().blackbox.info(Channel::Core, format!("segurança: {safety:?}"));
        self.shared().set_safety(safety);
    }

    pub fn telemetry(&self) -> Telemetry {
        self.shared().telemetry()
    }

    pub fn video_policy(&self) -> VideoPolicy {
        self.shared().video_policy()
    }

    /// Troca a política de taxa do vídeo; os drivers aplicam em até 1 s.
    pub fn set_video_policy(&self, policy: VideoPolicy) {
        self.shared().blackbox.info(Channel::Core, format!("vídeo: {policy:?}"));
        self.shared().set_video_policy(policy);
    }

    pub fn is_flying(&self) -> bool {
        self.shared().is_flying()
    }

    pub fn blackbox(&self) -> &Arc<Blackbox> {
        &self.shared().blackbox
    }

    pub fn disconnect(&self) {
        if !self.shared().running.swap(false, Ordering::SeqCst) {
            return;
        }
        // Uma reconexão em andamento desiste no próximo passo do handshake.
        if let Some(t) = self.relink.lock().unwrap().take() {
            if t.thread().id() != thread::current().id() {
                let _ = t.join();
            }
        }
        if let Some(driver) = self.session.driver.write().unwrap().take() {
            driver.shutdown();
        }
        self.shared().finish_flight(true);
        self.shared().blackbox.info(Channel::Core, "desconectado");
        self.shared().emit(DroneEvent::Disconnected { reason: "desconectado pelo usuário".into() });
    }
}

impl Drop for Drone {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl Session {
    fn driver(&self) -> Option<Arc<dyn Driver>> {
        self.driver.read().unwrap().clone()
    }

    /// Thread `tello-relink`: refaz a conexão quando a telemetria some.
    fn watch(&self) {
        while self.shared.running() {
            thread::sleep(WATCH_TICK);
            if self.shared.link_lost_for().is_some_and(|d| d >= RELINK_AFTER) {
                self.relink();
            }
        }
    }

    fn relink(&self) {
        if let Some(old) = self.driver.write().unwrap().take() {
            self.shared.blackbox.info(Channel::Core, "link perdido: refazendo a conexão");
            old.shutdown();
        }
        let mut attempt = 0;
        while self.shared.running() {
            attempt += 1;
            self.shared.emit(DroneEvent::Reconnecting { attempt });
            match open(&self.config, self.protocol, &self.shared) {
                Ok(driver) => return self.install(driver),
                Err(e) => self.shared.blackbox.info(Channel::Core, format!("reconexão {attempt}: {e}")),
            }
            let next = Instant::now() + RETRY_EVERY;
            while self.shared.running() && Instant::now() < next {
                thread::sleep(WATCH_TICK);
            }
        }
    }

    /// Põe o driver novo no lugar e devolve o que estava ligado antes da queda.
    fn install(&self, driver: Arc<dyn Driver>) {
        self.shared.link_reconnected();
        let restore = |action: Action| {
            if let Err(e) = driver.action(&action) {
                self.shared.blackbox.info(Channel::Core, format!("ao reconectar, {action:?} falhou: {e}"));
            }
        };
        if self.shared.video_wanted() {
            restore(Action::StreamOn);
        }
        if self.shared.telemetry().fast_mode == Some(true) {
            restore(Action::SetFastMode { on: true });
        }
        let caps = driver.caps();
        {
            let mut slot = self.driver.write().unwrap();
            if !self.shared.running() {
                drop(slot);
                driver.shutdown();
                return;
            }
            *slot = Some(driver);
        }
        *self.caps.lock().unwrap() = caps.clone();
        self.shared.blackbox.info(Channel::Core, format!("reconectado: {caps:?}"));
        self.shared.emit(DroneEvent::Connected { caps });
    }
}
