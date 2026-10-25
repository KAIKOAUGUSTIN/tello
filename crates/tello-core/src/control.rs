//! Estado compartilhado entre o handle público e os drivers: sticks,
//! limites de segurança, telemetria, saúde do link e o gravador do voo.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::blackbox::{Blackbox, Channel};
use crate::conditions::ConditionMonitor;
use crate::report::{FlightRecorder, ReportOptions};
use crate::util::unix_millis;
use crate::{DroneEvent, EventSink, Safety, Sticks, Telemetry, VideoPolicy, VideoStats, WarningCode};

/// Margem abaixo da altura máxima a partir da qual subir é bloqueado.
const HEIGHT_MARGIN_CM: i32 = 10;
pub(crate) const LINK_TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) struct Shared {
    pub sink: EventSink,
    pub blackbox: Arc<Blackbox>,
    pub running: AtomicBool,
    pub flying: AtomicBool,
    /// O usuário ligou o vídeo: religar depois de uma reconexão.
    video_wanted: AtomicBool,
    sticks: Mutex<(Sticks, Instant)>,
    safety: Mutex<Safety>,
    video: Mutex<VideoPolicy>,
    telemetry: Mutex<Telemetry>,
    link: Mutex<LinkState>,
    conditions: Mutex<ConditionMonitor>,
    recorder: Mutex<FlightRecorder>,
    auto_land_pending: AtomicBool,
    auto_land_done: AtomicBool,
}

#[derive(Default)]
struct LinkState {
    connected_at: Option<Instant>,
    last_state: Option<Instant>,
    lost: bool,
    warned_no_state: bool,
}

impl Shared {
    pub fn new(sink: EventSink, blackbox: Arc<Blackbox>, safety: Safety) -> Self {
        Shared {
            sink,
            blackbox,
            running: AtomicBool::new(true),
            flying: AtomicBool::new(false),
            video_wanted: AtomicBool::new(false),
            sticks: Mutex::new((Sticks::NEUTRAL, Instant::now())),
            safety: Mutex::new(safety),
            video: Mutex::new(VideoPolicy::default()),
            telemetry: Mutex::new(Telemetry::default()),
            link: Mutex::new(LinkState::default()),
            conditions: Mutex::new(ConditionMonitor::new()),
            recorder: Mutex::new(FlightRecorder::default()),
            auto_land_pending: AtomicBool::new(false),
            auto_land_done: AtomicBool::new(false),
        }
    }

    pub fn running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn emit(&self, event: DroneEvent) {
        (self.sink)(event)
    }

    pub fn warn(&self, code: WarningCode, message: impl Into<String>) {
        let message = message.into();
        self.blackbox.info(Channel::Core, format!("aviso {code:?}: {message}"));
        self.recorder.lock().unwrap().warning(code, &message, Instant::now());
        self.emit(DroneEvent::Warning { code, message });
    }

    /// Resumo do vídeo do último segundo (vai também para o relatório do voo).
    pub fn video_stats(&self, stats: VideoStats) {
        self.recorder.lock().unwrap().video(&stats, Instant::now());
        self.emit(DroneEvent::VideoStats(stats));
    }

    pub fn set_report_options(&self, options: ReportOptions) {
        self.recorder.lock().unwrap().options = options;
    }

    pub fn set_sticks(&self, s: Sticks) {
        *self.sticks.lock().unwrap() = (s.clamped(), Instant::now());
    }

    pub fn safety(&self) -> Safety {
        self.safety.lock().unwrap().clone()
    }

    pub fn set_safety(&self, s: Safety) {
        *self.safety.lock().unwrap() = s;
    }

    pub fn video_policy(&self) -> VideoPolicy {
        *self.video.lock().unwrap()
    }

    pub fn set_video_policy(&self, p: VideoPolicy) {
        *self.video.lock().unwrap() = p;
    }

    pub fn telemetry(&self) -> Telemetry {
        self.telemetry.lock().unwrap().clone()
    }

    pub fn set_flying(&self, flying: bool) {
        let was = self.flying.swap(flying, Ordering::Relaxed);
        if !flying {
            self.auto_land_pending.store(false, Ordering::Relaxed);
        } else {
            self.auto_land_done.store(false, Ordering::Relaxed);
        }
        match (was, flying) {
            (false, true) => self.start_flight(),
            (true, false) => self.finish_flight(false),
            _ => {}
        }
    }

    fn start_flight(&self) {
        let active = self.conditions.lock().unwrap().active();
        let lost = self.link.lock().unwrap().lost;
        self.recorder.lock().unwrap().start(Instant::now(), unix_millis(), &active, lost);
    }

    /// Fecha o relatório do voo em andamento, salva o HTML e avisa a UI.
    pub fn finish_flight(&self, interrupted: bool) {
        let (report, dir) = {
            let mut recorder = self.recorder.lock().unwrap();
            (recorder.finish(Instant::now(), interrupted), recorder.options.dir.clone())
        };
        let Some(report) = report else { return };
        let path = dir.and_then(|dir| {
            let path = dir.join(report.file_name());
            match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, report.render_html())) {
                Ok(()) => Some(path.display().to_string()),
                Err(e) => {
                    self.blackbox.info(Channel::Core, format!("não foi possível salvar o relatório do voo: {e}"));
                    None
                }
            }
        });
        let summary = report.summary;
        self.blackbox.info(Channel::Core, format!("relatório do voo: {summary:?} em {path:?}"));
        self.emit(DroneEvent::FlightReport { path, summary });
    }

    /// Condições de voo ativas agora.
    pub fn conditions(&self) -> Vec<crate::Condition> {
        self.conditions.lock().unwrap().active()
    }

    pub fn is_flying(&self) -> bool {
        self.flying.load(Ordering::Relaxed)
    }

    pub fn set_video_wanted(&self, on: bool) {
        self.video_wanted.store(on, Ordering::Relaxed);
    }

    pub fn video_wanted(&self) -> bool {
        self.video_wanted.load(Ordering::Relaxed)
    }

    /// Sticks que devem ser enviados agora, já com todos os limites aplicados.
    pub fn effective_sticks(&self) -> Sticks {
        let safety = self.safety();
        let (raw, at) = *self.sticks.lock().unwrap();
        if at.elapsed() > Duration::from_millis(safety.stick_timeout_ms as u64) {
            return Sticks::NEUTRAL;
        }
        let mut s = raw.clamped().scaled(safety.stick_limit.clamp(0.0, 1.0));
        if let Some(max) = safety.max_height_cm {
            let height = self.telemetry.lock().unwrap().height_cm;
            if let Some(h) = height {
                if h >= max as i32 - HEIGHT_MARGIN_CM && s.throttle > 0.0 {
                    s.throttle = 0.0;
                }
            }
        }
        s
    }

    pub fn mark_connected(&self) {
        let mut link = self.link.lock().unwrap();
        link.connected_at = Some(Instant::now());
    }

    /// Recebeu um pacote de estado/telemetria do drone (conta como sinal de link).
    pub fn update_telemetry(&self, update: impl FnOnce(&mut Telemetry)) {
        self.apply_telemetry(update, true);
    }

    /// Atualiza a telemetria com dados de outra origem (ex.: resposta a `battery?`).
    pub fn patch_telemetry(&self, update: impl FnOnce(&mut Telemetry)) {
        self.apply_telemetry(update, false);
    }

    fn apply_telemetry(&self, update: impl FnOnce(&mut Telemetry), from_link: bool) {
        let snapshot = {
            let mut t = self.telemetry.lock().unwrap();
            update(&mut t);
            t.clone()
        };
        let restored = from_link && {
            let mut link = self.link.lock().unwrap();
            link.last_state = Some(Instant::now());
            std::mem::replace(&mut link.lost, false)
        };
        if from_link {
            let mut recorder = self.recorder.lock().unwrap();
            if restored {
                recorder.link(false, Instant::now());
            }
            recorder.sample(&snapshot, Instant::now());
        }
        if restored {
            self.warn(WarningCode::LinkRestored, "Telemetria voltou a chegar.");
        }
        self.check_battery(&snapshot);
        self.check_conditions(&snapshot);
        self.emit(DroneEvent::Telemetry { data: snapshot });
    }

    fn check_conditions(&self, t: &Telemetry) {
        let (update, active) = {
            let mut monitor = self.conditions.lock().unwrap();
            let update = monitor.update(t, self.is_flying(), Instant::now());
            (update, monitor.active())
        };
        for c in update.warn {
            self.warn(c.warning(), c.message());
        }
        if update.changed {
            self.recorder.lock().unwrap().conditions(&active, Instant::now());
            self.blackbox.info(Channel::Core, format!("condições ativas: {active:?}"));
            self.emit(DroneEvent::Conditions { active });
        }
    }

    fn check_battery(&self, t: &Telemetry) {
        let (Some(bat), Some(limit)) = (t.battery, self.safety().auto_land_battery) else {
            return;
        };
        if bat <= limit && self.is_flying() && !self.auto_land_done.swap(true, Ordering::Relaxed) {
            self.auto_land_pending.store(true, Ordering::Relaxed);
            self.warn(WarningCode::AutoLand, format!("Bateria em {bat}%: pousando automaticamente."));
        }
    }

    /// O driver chama isto no seu loop; devolve true uma única vez por pedido.
    pub fn take_auto_land(&self) -> bool {
        self.auto_land_pending.swap(false, Ordering::Relaxed)
    }

    /// Há quanto tempo a telemetria parou, se o link foi dado como perdido.
    pub fn link_lost_for(&self) -> Option<Duration> {
        let link = self.link.lock().unwrap();
        link.last_state.filter(|_| link.lost).map(|t| t.elapsed())
    }

    /// Um driver novo conectou: o relógio do link recomeça, mas ele só volta
    /// a contar como restabelecido quando a telemetria chegar.
    pub fn link_reconnected(&self) {
        self.link.lock().unwrap().last_state = Some(Instant::now());
    }

    /// Verifica a saúde do link; chamado periodicamente pelo loop de controle.
    pub fn check_link(&self, no_state_hint: &str) {
        let mut link = self.link.lock().unwrap();
        let Some(connected_at) = link.connected_at else { return };
        match link.last_state {
            None if connected_at.elapsed() > LINK_TIMEOUT && !link.warned_no_state => {
                link.warned_no_state = true;
                drop(link);
                self.warn(WarningCode::NoState, no_state_hint.to_string());
            }
            Some(last) if last.elapsed() > LINK_TIMEOUT && !link.lost => {
                link.lost = true;
                drop(link);
                self.recorder.lock().unwrap().link(true, Instant::now());
                self.warn(WarningCode::LinkLost, "A telemetria parou de chegar. Verifique o Wi-Fi do drone.");
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared(safety: Safety) -> Shared {
        Shared::new(Arc::new(|_| {}), Arc::new(Blackbox::new(100)), safety)
    }

    #[test]
    fn applies_stick_limit() {
        let s = shared(Safety::beginner());
        s.set_sticks(Sticks { roll: 1.0, pitch: -1.0, throttle: 0.5, yaw: 2.0 });
        let e = s.effective_sticks();
        assert!((e.roll - 0.4).abs() < 1e-6);
        assert!((e.pitch + 0.4).abs() < 1e-6);
        assert!((e.yaw - 0.4).abs() < 1e-6, "clamp antes de escalar");
    }

    #[test]
    fn stale_sticks_return_to_neutral() {
        let mut safety = Safety::expert();
        safety.stick_timeout_ms = 0;
        let s = shared(safety);
        s.set_sticks(Sticks { roll: 1.0, ..Sticks::NEUTRAL });
        std::thread::sleep(Duration::from_millis(5));
        assert!(s.effective_sticks().is_neutral());
    }

    #[test]
    fn blocks_climb_at_max_height() {
        let s = shared(Safety { max_height_cm: Some(200), ..Safety::expert() });
        s.update_telemetry(|t| t.height_cm = Some(195));
        s.set_sticks(Sticks { throttle: 1.0, ..Sticks::NEUTRAL });
        assert_eq!(s.effective_sticks().throttle, 0.0);
        s.set_sticks(Sticks { throttle: -1.0, ..Sticks::NEUTRAL });
        assert_eq!(s.effective_sticks().throttle, -1.0);
    }

    #[test]
    fn auto_land_fires_once_while_flying() {
        let s = shared(Safety::beginner());
        s.update_telemetry(|t| t.battery = Some(8));
        assert!(!s.take_auto_land(), "no chão não pousa");
        s.set_flying(true);
        s.update_telemetry(|t| t.battery = Some(9));
        assert!(s.take_auto_land());
        s.update_telemetry(|t| t.battery = Some(8));
        assert!(!s.take_auto_land());
    }
}
