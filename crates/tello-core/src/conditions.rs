//! Condições de voo detectadas pela telemetria que explicam a maior parte da
//! instabilidade: pouca luz, vento, sinal fraco e altura acima do alcance da
//! câmera de baixo. Cada uma só liga ou desliga depois de se manter por um
//! tempo, para os avisos não piscarem.

use std::time::{Duration, Instant};

use crate::{Condition, Telemetry};

/// Tempo que a condição precisa durar para ligar (ou sumir para desligar).
const SETTLE: Duration = Duration::from_secs(3);
/// Depois de desligada, a condição só avisa de novo após este tempo.
const REARM: Duration = Duration::from_secs(30);
/// Sinal Wi-Fi (%) abaixo do qual o aviso liga...
pub const WEAK_SIGNAL: u8 = 30;
/// ...e a partir do qual ele desliga.
pub const SIGNAL_OK: u8 = 40;
/// Alcance da câmera de baixo (VPS): acima disso o drone deriva.
pub const VPS_MAX_CM: i32 = 600;

const ALL: [Condition; 4] = [Condition::LowLight, Condition::Wind, Condition::WeakSignal, Condition::HighAltitude];

#[derive(Clone, Copy, Debug, Default)]
struct Debounce {
    active: bool,
    /// Desde quando o valor lido difere do estado atual.
    pending_since: Option<Instant>,
    cleared_at: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    /// Ligou; `warn` é falso quando ela tinha desligado há pouco.
    On {
        warn: bool,
    },
    Off,
}

impl Debounce {
    /// `raw` = o valor lido agora; `None` quando a telemetria não informa.
    fn update(&mut self, raw: Option<bool>, now: Instant) -> Option<Change> {
        let Some(raw) = raw.filter(|&r| r != self.active) else {
            self.pending_since = None;
            return None;
        };
        let since = *self.pending_since.get_or_insert(now);
        if now.duration_since(since) < SETTLE {
            return None;
        }
        self.pending_since = None;
        self.active = raw;
        if raw {
            let warn = !matches!(self.cleared_at, Some(t) if now.duration_since(t) < REARM);
            Some(Change::On { warn })
        } else {
            self.cleared_at = Some(now);
            Some(Change::Off)
        }
    }
}

/// Resultado de uma atualização do monitor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Update {
    /// Condições que acabaram de ligar e devem gerar aviso.
    pub warn: Vec<Condition>,
    /// O conjunto de condições ativas mudou.
    pub changed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ConditionMonitor {
    state: [Debounce; ALL.len()],
}

impl ConditionMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, t: &Telemetry, flying: bool, now: Instant) -> Update {
        let mut out = Update::default();
        for (i, cond) in ALL.into_iter().enumerate() {
            let active = self.state[i].active;
            let raw = match cond {
                Condition::LowLight => t.light_strength.map(|l| l != 0),
                Condition::Wind => t.wind.map(|w| w && flying),
                // Histerese: liga abaixo de 30% e só desliga a partir de 40%.
                Condition::WeakSignal => t.wifi_strength.map(|w| w < if active { SIGNAL_OK } else { WEAK_SIGNAL }),
                Condition::HighAltitude => t.height_cm.map(|h| flying && h > VPS_MAX_CM),
            };
            match self.state[i].update(raw, now) {
                Some(Change::On { warn }) => {
                    out.changed = true;
                    if warn {
                        out.warn.push(cond);
                    }
                }
                Some(Change::Off) => out.changed = true,
                None => {}
            }
        }
        out
    }

    pub fn active(&self) -> Vec<Condition> {
        ALL.into_iter().zip(&self.state).filter(|(_, d)| d.active).map(|(c, _)| c).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, secs: f32) -> Instant {
        start + Duration::from_secs_f32(secs)
    }

    #[test]
    fn debounce_needs_the_condition_to_last() {
        let t0 = Instant::now();
        let mut d = Debounce::default();
        assert_eq!(d.update(Some(true), at(t0, 0.0)), None);
        assert_eq!(d.update(Some(true), at(t0, 2.0)), None);
        // Uma leitura boa no meio recomeça a contagem.
        assert_eq!(d.update(Some(false), at(t0, 2.5)), None);
        assert_eq!(d.update(Some(true), at(t0, 3.0)), None);
        assert_eq!(d.update(Some(true), at(t0, 6.0)), Some(Change::On { warn: true }));
        assert_eq!(d.update(None, at(t0, 7.0)), None, "sem dado mantém o estado");
        assert_eq!(d.update(Some(false), at(t0, 8.0)), None);
        assert_eq!(d.update(Some(false), at(t0, 11.0)), Some(Change::Off));
    }

    #[test]
    fn does_not_warn_again_right_after_clearing() {
        let t0 = Instant::now();
        let mut d = Debounce::default();
        d.update(Some(true), at(t0, 0.0));
        assert_eq!(d.update(Some(true), at(t0, 3.0)), Some(Change::On { warn: true }));
        d.update(Some(false), at(t0, 4.0));
        assert_eq!(d.update(Some(false), at(t0, 7.0)), Some(Change::Off));
        d.update(Some(true), at(t0, 8.0));
        assert_eq!(d.update(Some(true), at(t0, 11.0)), Some(Change::On { warn: false }));
        d.update(Some(false), at(t0, 12.0));
        d.update(Some(false), at(t0, 15.0));
        d.update(Some(true), at(t0, 50.0));
        assert_eq!(d.update(Some(true), at(t0, 53.0)), Some(Change::On { warn: true }));
    }

    fn feed(m: &mut ConditionMonitor, t: &Telemetry, flying: bool, t0: Instant, until: f32) -> Vec<Condition> {
        let mut warned = Vec::new();
        let mut s = 0.0;
        while s <= until {
            warned.extend(m.update(t, flying, at(t0, s)).warn);
            s += 0.5;
        }
        warned
    }

    #[test]
    fn detects_each_condition() {
        let t0 = Instant::now();
        let mut m = ConditionMonitor::new();
        let t = Telemetry {
            light_strength: Some(1),
            wind: Some(true),
            wifi_strength: Some(20),
            height_cm: Some(700),
            ..Default::default()
        };
        let warned = feed(&mut m, &t, true, t0, 4.0);
        assert_eq!(warned, ALL.to_vec());
        assert_eq!(m.active(), ALL.to_vec());
    }

    #[test]
    fn wind_and_altitude_only_count_in_flight() {
        let t0 = Instant::now();
        let mut m = ConditionMonitor::new();
        let t = Telemetry { wind: Some(true), height_cm: Some(700), ..Default::default() };
        assert!(feed(&mut m, &t, false, t0, 5.0).is_empty());
        assert!(m.active().is_empty());
    }

    #[test]
    fn weak_signal_has_hysteresis() {
        let t0 = Instant::now();
        let mut m = ConditionMonitor::new();
        let weak = Telemetry { wifi_strength: Some(25), ..Default::default() };
        feed(&mut m, &weak, false, t0, 4.0);
        assert_eq!(m.active(), [Condition::WeakSignal]);
        // 35% ainda não basta para desligar.
        let mid = Telemetry { wifi_strength: Some(35), ..Default::default() };
        feed(&mut m, &mid, false, at(t0, 5.0), 5.0);
        assert_eq!(m.active(), [Condition::WeakSignal]);
        let ok = Telemetry { wifi_strength: Some(60), ..Default::default() };
        let changed = (0..10).any(|i| m.update(&ok, false, at(t0, 11.0 + i as f32 * 0.5)).changed);
        assert!(changed);
        assert!(m.active().is_empty());
    }

    #[test]
    fn missing_data_never_triggers() {
        let t0 = Instant::now();
        let mut m = ConditionMonitor::new();
        assert!(feed(&mut m, &Telemetry::default(), true, t0, 10.0).is_empty());
    }
}
