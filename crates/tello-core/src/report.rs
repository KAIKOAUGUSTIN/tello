//! Relatório do voo: a telemetria é amostrada da decolagem ao pouso e, no fim,
//! vira um HTML autocontido com o resumo, os gráficos e as possíveis causas de
//! instabilidade.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::conditions::WEAK_SIGNAL;
use crate::util::from_unix_millis;
use crate::{Condition, Telemetry, VideoStats, WarningCode};

const SAMPLE_EVERY: Duration = Duration::from_secs(1);
/// Voos mais curtos que isso são ruído (ex.: o `em_sky` oscilando na decolagem).
const MIN_FLIGHT: Duration = Duration::from_secs(2);
/// Limites de memória: 2 h de amostras (um voo do Tello dura uns 13 min).
const MAX_SAMPLES: usize = 7200;
const MAX_WARNINGS: usize = 200;
/// Um resumo de vídeo mais velho que isso não vale para a amostra.
const VIDEO_STALE: Duration = Duration::from_secs(2);

const HOT_C: i32 = 85;
const LOW_BATTERY: u8 = 15;
const HIGH_VIDEO_LOSS: f32 = 5.0;

/// Onde salvar o relatório e como mostrar a hora.
#[derive(Clone, Debug, Default)]
pub struct ReportOptions {
    pub dir: Option<PathBuf>,
    /// Fuso do usuário, em minutos a partir do UTC (ex.: -180 em Brasília).
    pub utc_offset_min: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Sample {
    pub t_s: f32,
    pub battery: Option<u8>,
    pub height_cm: Option<i32>,
    pub wifi: Option<u8>,
    pub temp_c: Option<i32>,
    pub video_loss_pct: Option<f32>,
}

/// O que atrapalhou o voo durante um intervalo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Trouble {
    Condition(Condition),
    LinkLost,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub trouble: Trouble,
    pub start_s: f32,
    pub end_s: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoggedWarning {
    pub t_s: f32,
    pub code: WarningCode,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct FlightSummary {
    /// Início, em ms desde 1970 (UTC).
    pub started_unix_ms: i64,
    pub duration_s: u32,
    /// Terminou sem pouso: a conexão foi encerrada com o drone no ar.
    pub interrupted: bool,
    pub battery_start: Option<u8>,
    pub battery_end: Option<u8>,
    pub max_height_cm: Option<i32>,
    pub wifi_min: Option<u8>,
    pub wifi_avg: Option<u8>,
    pub video_loss_avg: Option<f32>,
    pub temp_max_c: Option<i32>,
    pub warnings: u32,
    pub link_lost_s: u32,
    /// Possíveis causas de instabilidade, da mais provável para a menos.
    pub causes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct FlightReport {
    pub summary: FlightSummary,
    pub samples: Vec<Sample>,
    pub spans: Vec<Span>,
    pub warnings: Vec<LoggedWarning>,
    pub utc_offset_min: i32,
}

#[derive(Default)]
pub struct FlightRecorder {
    pub options: ReportOptions,
    flight: Option<Flight>,
}

struct Flight {
    started: Instant,
    started_unix_ms: i64,
    samples: Vec<Sample>,
    last_sample: Option<Instant>,
    video: Option<(Instant, f32)>,
    video_loss: (f64, u32),
    warnings: Vec<LoggedWarning>,
    warning_count: u32,
    open: HashMap<Trouble, f32>,
    spans: Vec<Span>,
}

impl Flight {
    fn t(&self, now: Instant) -> f32 {
        now.saturating_duration_since(self.started).as_secs_f32()
    }

    fn open(&mut self, trouble: Trouble, now: Instant) {
        let t = self.t(now);
        self.open.entry(trouble).or_insert(t);
    }

    fn close(&mut self, trouble: Trouble, now: Instant) {
        if let Some(start_s) = self.open.remove(&trouble) {
            self.spans.push(Span { trouble, start_s, end_s: self.t(now) });
        }
    }
}

impl FlightRecorder {
    pub fn new(options: ReportOptions) -> Self {
        FlightRecorder { options, flight: None }
    }

    pub fn is_recording(&self) -> bool {
        self.flight.is_some()
    }

    /// Decolou. Condições já ativas e o link já perdido contam desde o início.
    pub fn start(&mut self, now: Instant, unix_ms: i64, active: &[Condition], link_lost: bool) {
        let mut f = Flight {
            started: now,
            started_unix_ms: unix_ms,
            samples: Vec::new(),
            last_sample: None,
            video: None,
            video_loss: (0.0, 0),
            warnings: Vec::new(),
            warning_count: 0,
            open: HashMap::new(),
            spans: Vec::new(),
        };
        for &c in active {
            f.open(Trouble::Condition(c), now);
        }
        if link_lost {
            f.open(Trouble::LinkLost, now);
        }
        self.flight = Some(f);
    }

    /// Guarda uma amostra por segundo da telemetria que chega.
    pub fn sample(&mut self, t: &Telemetry, now: Instant) {
        let Some(f) = self.flight.as_mut() else { return };
        if f.last_sample.is_some_and(|last| now.saturating_duration_since(last) < SAMPLE_EVERY)
            || f.samples.len() >= MAX_SAMPLES
        {
            return;
        }
        f.last_sample = Some(now);
        let video_loss_pct = f.video.filter(|(at, _)| now.saturating_duration_since(*at) < VIDEO_STALE).map(|v| v.1);
        let sample = Sample {
            t_s: f.t(now),
            battery: t.battery,
            height_cm: t.height_cm,
            wifi: t.wifi_strength,
            temp_c: t.temp_high_c,
            video_loss_pct,
        };
        f.samples.push(sample);
    }

    pub fn video(&mut self, s: &VideoStats, now: Instant) {
        let Some(f) = self.flight.as_mut() else { return };
        if let Some(loss) = s.loss_pct {
            f.video = Some((now, loss));
            f.video_loss.0 += f64::from(loss);
            f.video_loss.1 += 1;
        }
    }

    pub fn warning(&mut self, code: WarningCode, message: &str, now: Instant) {
        let Some(f) = self.flight.as_mut() else { return };
        if code != WarningCode::LinkRestored {
            f.warning_count += 1;
        }
        if f.warnings.len() < MAX_WARNINGS {
            let t_s = f.t(now);
            f.warnings.push(LoggedWarning { t_s, code, message: message.to_string() });
        }
    }

    pub fn conditions(&mut self, active: &[Condition], now: Instant) {
        let Some(f) = self.flight.as_mut() else { return };
        let ended: Vec<Trouble> =
            f.open.keys().copied().filter(|t| matches!(t, Trouble::Condition(c) if !active.contains(c))).collect();
        for t in ended {
            f.close(t, now);
        }
        for &c in active {
            f.open(Trouble::Condition(c), now);
        }
    }

    pub fn link(&mut self, lost: bool, now: Instant) {
        let Some(f) = self.flight.as_mut() else { return };
        if lost {
            f.open(Trouble::LinkLost, now);
        } else {
            f.close(Trouble::LinkLost, now);
        }
    }

    /// Pousou (ou desconectou no ar). Devolve o relatório, exceto de voos curtos demais.
    pub fn finish(&mut self, now: Instant, interrupted: bool) -> Option<FlightReport> {
        let mut f = self.flight.take()?;
        if now.saturating_duration_since(f.started) < MIN_FLIGHT {
            return None;
        }
        let open: Vec<Trouble> = f.open.keys().copied().collect();
        for t in open {
            f.close(t, now);
        }
        f.spans.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
        let duration_s = f.t(now);
        let summary = summarize(&f, duration_s, interrupted);
        Some(FlightReport {
            summary,
            samples: f.samples,
            spans: f.spans,
            warnings: f.warnings,
            utc_offset_min: self.options.utc_offset_min,
        })
    }
}

fn total(spans: &[Span], trouble: Trouble) -> f32 {
    spans.iter().filter(|s| s.trouble == trouble).map(|s| s.end_s - s.start_s).sum()
}

fn summarize(f: &Flight, duration_s: f32, interrupted: bool) -> FlightSummary {
    let s = &f.samples;
    let battery: Vec<u8> = s.iter().filter_map(|x| x.battery).collect();
    let wifi: Vec<u8> = s.iter().filter_map(|x| x.wifi).collect();
    let mut sum = FlightSummary {
        started_unix_ms: f.started_unix_ms,
        duration_s: duration_s.round() as u32,
        interrupted,
        battery_start: battery.first().copied(),
        battery_end: battery.last().copied(),
        max_height_cm: s.iter().filter_map(|x| x.height_cm).max(),
        wifi_min: wifi.iter().min().copied(),
        wifi_avg: (!wifi.is_empty())
            .then(|| (wifi.iter().map(|&w| u32::from(w)).sum::<u32>() as f32 / wifi.len() as f32).round() as u8),
        video_loss_avg: (f.video_loss.1 > 0).then(|| (f.video_loss.0 / f64::from(f.video_loss.1)) as f32),
        temp_max_c: s.iter().filter_map(|x| x.temp_c).max(),
        warnings: f.warning_count,
        link_lost_s: total(&f.spans, Trouble::LinkLost).round() as u32,
        causes: Vec::new(),
    };
    sum.causes = causes(&sum, &f.spans, duration_s);
    sum
}

fn causes(s: &FlightSummary, spans: &[Span], duration_s: f32) -> Vec<String> {
    let secs = |t: Trouble| total(spans, t).round() as u32;
    let mut out = Vec::new();
    if s.link_lost_s >= 1 {
        out.push(format!(
            "A conexão caiu por {} no total. Voe mais perto e sem paredes ou árvores entre você e o drone.",
            duration(s.link_lost_s)
        ));
    }
    let low_light = total(spans, Trouble::Condition(Condition::LowLight));
    if duration_s > 0.0 && low_light / duration_s >= 0.1 {
        out.push(format!(
            "Pouca luz para a câmera de baixo em {:.0}% do voo: o drone perde a referência do chão e deriva. \
             Voe num lugar mais claro e sobre um chão com textura.",
            low_light / duration_s * 100.0
        ));
    }
    let wind = secs(Trouble::Condition(Condition::Wind));
    if wind >= 3 {
        out.push(format!("Vento forte por {}: o drone deriva e gasta mais bateria para se segurar.", duration(wind)));
    }
    let high = secs(Trouble::Condition(Condition::HighAltitude));
    if high >= 3 {
        out.push(format!("Acima de 6 m por {}, onde a câmera de baixo não enxerga o chão.", duration(high)));
    }
    let weak = secs(Trouble::Condition(Condition::WeakSignal));
    if weak >= 5 || s.wifi_min.is_some_and(|w| w < WEAK_SIGNAL) {
        let min = s.wifi_min.map(|w| format!(" O mínimo foi {w}%.")).unwrap_or_default();
        let span = if weak >= 1 { format!(" por {}", duration(weak)) } else { String::new() };
        out.push(format!("Sinal Wi-Fi fraco{span}.{min} Comandos e vídeo atrasam com sinal baixo."));
    }
    if let Some(loss) = s.video_loss_avg.filter(|&l| l > HIGH_VIDEO_LOSS) {
        out.push(format!(
            "O vídeo perdeu em média {} dos pacotes. Escolha uma qualidade menor em Configurações → Vídeo \
             ou voe mais perto.",
            pct(loss)
        ));
    }
    if let Some(t) = s.temp_max_c.filter(|&t| t >= HOT_C) {
        out.push(format!(
            "A temperatura chegou a {t} °C. O Tello esquenta rápido parado no chão com o vídeo ligado; \
             deixe esfriar entre os voos."
        ));
    }
    if let Some(b) = s.battery_end.filter(|&b| b <= LOW_BATTERY) {
        out.push(format!("A bateria terminou em {b}%: com carga baixa o drone perde força e pousa sozinho."));
    }
    if s.interrupted {
        out.push("O voo terminou sem pouso registrado: a conexão foi encerrada com o drone no ar.".into());
    }
    out
}

// --- HTML -------------------------------------------------------------------

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Número com vírgula decimal.
fn dec(v: f32, places: usize) -> String {
    format!("{v:.places$}").replace('.', ",")
}

fn pct(v: f32) -> String {
    format!("{}%", dec(v, if v < 10.0 { 1 } else { 0 }))
}

fn duration(secs: u32) -> String {
    match (secs / 60, secs % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s} s"),
    }
}

fn clock(t_s: f32) -> String {
    let t = t_s.max(0.0).round() as u32;
    format!("{}:{:02}", t / 60, t % 60)
}

fn offset_label(min: i32) -> String {
    if min == 0 {
        return "UTC".into();
    }
    let sign = if min < 0 { '−' } else { '+' };
    format!("UTC{sign}{:02}:{:02}", min.abs() / 60, min.abs() % 60)
}

fn trouble_label(t: Trouble) -> &'static str {
    match t {
        Trouble::Condition(Condition::LowLight) => "Pouca luz",
        Trouble::Condition(Condition::Wind) => "Vento forte",
        Trouble::Condition(Condition::WeakSignal) => "Wi-Fi fraco",
        Trouble::Condition(Condition::HighAltitude) => "Acima de 6 m",
        Trouble::LinkLost => "Sem conexão",
    }
}

const TROUBLES: [Trouble; 5] = [
    Trouble::LinkLost,
    Trouble::Condition(Condition::LowLight),
    Trouble::Condition(Condition::Wind),
    Trouble::Condition(Condition::WeakSignal),
    Trouble::Condition(Condition::HighAltitude),
];

const CSS: &str = "\
:root{--bg:#f6f7f9;--card:#fff;--text:#14181f;--muted:#5b6575;--border:#dde2e8;--accent:#0b84c6;\
--warn:#b86e00;--danger:#c8323c;--ok:#1b8a5a;color-scheme:light dark}\
@media (prefers-color-scheme:dark){:root{--bg:#0b0f15;--card:#131a24;--text:#e7ecf2;--muted:#94a0b2;\
--border:#253041;--accent:#38bdf8;--warn:#f5a524;--danger:#f05660;--ok:#3ccf8e}}\
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);\
font:15px/1.5 system-ui,-apple-system,'Segoe UI',Roboto,sans-serif}\
main{max-width:860px;margin:0 auto;padding:24px 16px 48px}h1{font-size:24px;margin:0}\
h2{font-size:17px;margin:28px 0 10px}.sub{color:var(--muted);margin:4px 0 0}\
.cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:10px;margin-top:20px}\
.card{background:var(--card);border:1px solid var(--border);border-radius:10px;padding:10px 12px}\
.card b{display:block;font-size:19px}.card span{color:var(--muted);font-size:13px}\
ul.causes{margin:0;padding:0;list-style:none;display:grid;gap:8px}\
ul.causes li{background:var(--card);border:1px solid var(--border);border-left:3px solid var(--warn);\
border-radius:8px;padding:10px 12px}.none{color:var(--ok)}\
.chart{background:var(--card);border:1px solid var(--border);border-radius:10px;padding:10px 12px;margin-bottom:10px}\
.chart h3{font-size:14px;margin:0 0 6px;color:var(--muted);font-weight:600}\
.plot,.tl{display:grid;grid-template-columns:auto 1fr;column-gap:10px;font-size:12px;color:var(--muted)}\
.y{display:flex;flex-direction:column;justify-content:space-between;text-align:right;line-height:1;\
font-variant-numeric:tabular-nums}\
.plot svg{display:block;width:100%;height:110px;overflow:visible}\
.grid{stroke:var(--border);stroke-width:1;vector-effect:non-scaling-stroke}\
.line{fill:none;stroke:var(--accent);stroke-width:2;stroke-linejoin:round;stroke-linecap:round;\
vector-effect:non-scaling-stroke}\
.x{display:flex;justify-content:space-between;margin-top:4px;font-variant-numeric:tabular-nums}\
.tl{row-gap:6px;align-items:center;font-size:13px}.tl .x{font-size:12px}\
.track{position:relative;height:12px;border-radius:3px;background:var(--bg)}\
.track i{position:absolute;top:0;bottom:0;min-width:3px;border-radius:3px;background:var(--warn)}\
.track i.link{background:var(--danger)}\
table{width:100%;border-collapse:collapse;background:var(--card);border:1px solid var(--border);border-radius:10px}\
td{padding:8px 12px;border-top:1px solid var(--border);vertical-align:top}tr:first-child td{border-top:0}\
td.t{color:var(--muted);white-space:nowrap;font-variant-numeric:tabular-nums}\
footer{margin-top:32px;color:var(--muted);font-size:13px}";

/// Coordenadas do SVG: ele estica para a largura da página (os textos ficam
/// fora dele, em HTML, para não encolherem no celular).
const W: f32 = 1000.0;
const H: f32 = 100.0;

fn frac(t: f32, dur: f32) -> f32 {
    (t / dur.max(1.0)).clamp(0.0, 1.0)
}

/// Gráfico de linha; um valor ausente interrompe a linha.
fn line_chart(out: &mut String, title: &str, unit: &str, dur: f32, points: &[(f32, Option<f32>)], range: (f32, f32)) {
    let values: Vec<f32> = points.iter().filter_map(|p| p.1).collect();
    if values.is_empty() {
        return;
    }
    let lo = values.iter().copied().fold(range.0, f32::min);
    let mut hi = values.iter().copied().fold(range.1, f32::max);
    if hi - lo < 1e-3 {
        hi = lo + 1.0;
    }
    let y_of = |v: f32| (hi - v) / (hi - lo) * H;
    let places = if hi - lo < 10.0 { 1 } else { 0 };
    let _ = write!(
        out,
        "<div class=\"chart\"><h3>{}</h3><div class=\"plot\"><div class=\"y\"><span>{}{unit}</span><span>{}{unit}</span></div>\
         <svg viewBox=\"0 0 {W} {H}\" preserveAspectRatio=\"none\" role=\"img\" aria-label=\"{}\">\
         <line class=\"grid\" x1=\"0\" x2=\"{W}\" y1=\"0\" y2=\"0\"/><line class=\"grid\" x1=\"0\" x2=\"{W}\" y1=\"{H}\" y2=\"{H}\"/>",
        esc(title),
        dec(hi, places),
        dec(lo, places),
        esc(title),
        unit = esc(unit),
    );
    for run in points.split(|p| p.1.is_none()).filter(|r| !r.is_empty()) {
        let pts: Vec<String> =
            run.iter().map(|(t, v)| format!("{:.1},{:.1}", frac(*t, dur) * W, y_of(v.unwrap_or(lo)))).collect();
        let pts = if pts.len() == 1 { format!("{0} {0}", pts[0]) } else { pts.join(" ") };
        let _ = write!(out, "<polyline class=\"line\" points=\"{pts}\"/>");
    }
    let _ = write!(
        out,
        "</svg><span></span><div class=\"x\"><span>0:00</span><span>{}</span></div></div></div>",
        clock(dur)
    );
}

/// Faixas de tempo com cada problema.
fn timeline(out: &mut String, spans: &[Span], dur: f32) {
    let rows: Vec<Trouble> = TROUBLES.into_iter().filter(|t| spans.iter().any(|s| s.trouble == *t)).collect();
    if rows.is_empty() {
        return;
    }
    out.push_str("<div class=\"chart\"><h3>Linha do tempo</h3><div class=\"tl\">");
    for trouble in rows {
        let _ = write!(out, "<span>{}</span><div class=\"track\">", trouble_label(trouble));
        let class = if trouble == Trouble::LinkLost { " class=\"link\"" } else { "" };
        for s in spans.iter().filter(|s| s.trouble == trouble) {
            let (a, b) = (frac(s.start_s, dur), frac(s.end_s, dur));
            let _ = write!(
                out,
                "<i{class} style=\"left:{:.2}%;width:{:.2}%\" title=\"{} a {}\"></i>",
                a * 100.0,
                (b - a) * 100.0,
                clock(s.start_s),
                clock(s.end_s)
            );
        }
        out.push_str("</div>");
    }
    let _ = write!(out, "<span></span><div class=\"x\"><span>0:00</span><span>{}</span></div></div></div>", clock(dur));
}

impl FlightReport {
    /// Data e hora no fuso do usuário.
    fn local(&self) -> crate::util::DateTime {
        from_unix_millis(self.summary.started_unix_ms + i64::from(self.utc_offset_min) * 60_000)
    }

    /// `voo-AAAAMMDD-HHMMSS.html`, na hora local.
    pub fn file_name(&self) -> String {
        let t = self.local();
        format!("voo-{:04}{:02}{:02}-{:02}{:02}{:02}.html", t.year, t.month, t.day, t.hour, t.minute, t.second)
    }

    pub fn render_html(&self) -> String {
        let s = &self.summary;
        let t = self.local();
        let date = format!("{:02}/{:02}/{:04}", t.day, t.month, t.year);
        let time = format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second);
        let dur = s.duration_s as f32;
        let mut out = String::with_capacity(16 * 1024);
        let _ = write!(
            out,
            "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
             <title>Voo de {date} {}</title><style>{CSS}</style></head><body><main>\
             <h1>Relatório do voo</h1><p class=\"sub\">{date}, {time} ({}) · {}{}</p>",
            &time[..5],
            offset_label(self.utc_offset_min),
            duration(s.duration_s),
            if s.interrupted { " · interrompido" } else { "" }
        );

        out.push_str("<section class=\"cards\">");
        let mut card = |value: String, label: &str| {
            let _ = write!(out, "<div class=\"card\"><b>{}</b><span>{}</span></div>", esc(&value), esc(label));
        };
        card(duration(s.duration_s), "Duração");
        if let (Some(a), Some(b)) = (s.battery_start, s.battery_end) {
            card(format!("{a}% → {b}%"), "Bateria");
        }
        if let Some(h) = s.max_height_cm {
            card(format!("{} m", dec(h as f32 / 100.0, 1)), "Altura máxima");
        }
        if let (Some(min), Some(avg)) = (s.wifi_min, s.wifi_avg) {
            card(format!("{min}% / {avg}%"), "Wi-Fi mínimo / médio");
        }
        if let Some(loss) = s.video_loss_avg {
            card(pct(loss), "Perda média de vídeo");
        }
        if let Some(temp) = s.temp_max_c {
            card(format!("{temp} °C"), "Temperatura máxima");
        }
        card(s.warnings.to_string(), "Avisos");
        card(duration(s.link_lost_s), "Sem conexão");
        out.push_str("</section>");

        out.push_str("<h2>Possíveis causas</h2>");
        if s.causes.is_empty() {
            out.push_str("<p class=\"none\">Nada fora do normal foi registrado neste voo.</p>");
        } else {
            out.push_str("<ul class=\"causes\">");
            for c in &s.causes {
                let _ = write!(out, "<li>{}</li>", esc(c));
            }
            out.push_str("</ul>");
        }

        out.push_str("<h2>Gráficos</h2>");
        let series = |f: fn(&Sample) -> Option<f32>| -> Vec<(f32, Option<f32>)> {
            self.samples.iter().map(|x| (x.t_s, f(x))).collect()
        };
        timeline(&mut out, &self.spans, dur);
        line_chart(&mut out, "Bateria", "%", dur, &series(|x| x.battery.map(f32::from)), (0.0, 100.0));
        line_chart(&mut out, "Altura", " m", dur, &series(|x| x.height_cm.map(|h| h as f32 / 100.0)), (0.0, 1.0));
        line_chart(&mut out, "Sinal Wi-Fi", "%", dur, &series(|x| x.wifi.map(f32::from)), (0.0, 100.0));
        line_chart(&mut out, "Perda de vídeo", "%", dur, &series(|x| x.video_loss_pct), (0.0, 10.0));
        line_chart(&mut out, "Temperatura", " °C", dur, &series(|x| x.temp_c.map(|t| t as f32)), (f32::MAX, f32::MIN));

        if !self.warnings.is_empty() {
            out.push_str("<h2>Avisos</h2><table>");
            for w in &self.warnings {
                let _ = write!(out, "<tr><td class=\"t\">{}</td><td>{}</td></tr>", clock(w.t_s), esc(&w.message));
            }
            out.push_str("</table>");
        }
        out.push_str("<footer>Gerado pelo Tello Pilot.</footer></main></body></html>\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t0: Instant, secs: f32) -> Instant {
        t0 + Duration::from_secs_f32(secs)
    }

    fn telemetry(battery: u8, height_cm: i32, wifi: u8) -> Telemetry {
        Telemetry {
            battery: Some(battery),
            height_cm: Some(height_cm),
            wifi_strength: Some(wifi),
            temp_high_c: Some(60),
            ..Default::default()
        }
    }

    /// Telemetria a 10 Hz de `from` a `to` segundos.
    fn fly(r: &mut FlightRecorder, t0: Instant, from: f32, to: f32, wifi: u8) {
        for i in (from * 10.0).round() as u32..=(to * 10.0).round() as u32 {
            let s = i as f32 / 10.0;
            let battery = 90 - (s / 2.0) as u8;
            r.sample(&telemetry(battery, (s * 10.0) as i32, wifi), at(t0, s));
        }
    }

    #[test]
    fn samples_once_per_second_and_summarizes() {
        let t0 = Instant::now();
        let mut r = FlightRecorder::default();
        r.sample(&telemetry(99, 0, 90), t0);
        assert!(!r.is_recording(), "no chão nada é gravado");
        r.start(t0, 1_709_210_096_789, &[], false);
        fly(&mut r, t0, 0.0, 2.9, 80);
        r.video(&VideoStats { fps: 30, kbps: 2000, loss_pct: Some(2.0), level: Some(4) }, at(t0, 3.0));
        fly(&mut r, t0, 3.0, 3.9, 80);
        r.video(&VideoStats { fps: 30, kbps: 2000, loss_pct: Some(4.0), level: Some(4) }, at(t0, 4.0));
        r.warning(WarningCode::Wind, "vento", at(t0, 5.0));
        r.warning(WarningCode::LinkRestored, "voltou", at(t0, 6.0));
        fly(&mut r, t0, 4.0, 10.0, 80);
        let report = r.finish(at(t0, 10.1), false).expect("relatório");
        assert!(!r.is_recording());
        assert_eq!(report.samples.len(), 11);
        assert_eq!(report.samples[3].video_loss_pct, Some(2.0));
        assert_eq!(report.samples[5].video_loss_pct, Some(4.0));
        assert_eq!(report.samples[6].video_loss_pct, None, "resumo de vídeo velho");
        let s = &report.summary;
        assert_eq!(s.duration_s, 10);
        assert_eq!((s.battery_start, s.battery_end), (Some(90), Some(85)));
        assert_eq!(s.max_height_cm, Some(100));
        assert_eq!((s.wifi_min, s.wifi_avg), (Some(80), Some(80)));
        assert_eq!(s.video_loss_avg, Some(3.0));
        assert_eq!(s.warnings, 1, "o aviso de link restabelecido não conta");
        assert!(s.causes.is_empty(), "{:?}", s.causes);
        assert_eq!(report.file_name(), "voo-20240229-123456.html");
    }

    #[test]
    fn short_flights_are_discarded() {
        let t0 = Instant::now();
        let mut r = FlightRecorder::default();
        r.start(t0, 0, &[], false);
        assert!(r.finish(at(t0, 1.0), false).is_none());
        assert!(r.finish(at(t0, 5.0), false).is_none(), "sem voo em andamento");
    }

    #[test]
    fn spans_become_causes() {
        let t0 = Instant::now();
        let mut r = FlightRecorder::default();
        r.start(t0, 0, &[Condition::LowLight], false);
        fly(&mut r, t0, 0.0, 20.0, 25);
        r.conditions(&[Condition::LowLight, Condition::Wind], at(t0, 4.0));
        r.conditions(&[Condition::Wind], at(t0, 10.0));
        r.link(true, at(t0, 12.0));
        r.link(false, at(t0, 15.0));
        r.video(&VideoStats { loss_pct: Some(12.0), ..Default::default() }, at(t0, 16.0));
        let report = r.finish(at(t0, 20.0), true).unwrap();
        let low = report.spans.iter().find(|s| s.trouble == Trouble::Condition(Condition::LowLight)).unwrap();
        assert_eq!((low.start_s, low.end_s.round()), (0.0, 10.0));
        let wind = report.spans.iter().find(|s| s.trouble == Trouble::Condition(Condition::Wind)).unwrap();
        assert_eq!(wind.end_s.round(), 20.0, "fechada no fim do voo");
        let s = &report.summary;
        assert_eq!(s.link_lost_s, 3);
        let causes = s.causes.join("\n");
        for expected in [
            "conexão caiu por 3 s",
            "Pouca luz",
            "em 50% do voo",
            "Vento forte por 16 s",
            "Wi-Fi fraco",
            "12%",
            "sem pouso",
        ] {
            assert!(causes.contains(expected), "faltou '{expected}' em:\n{causes}");
        }
    }

    #[test]
    fn renders_self_contained_html() {
        let t0 = Instant::now();
        let mut r = FlightRecorder::new(ReportOptions { dir: None, utc_offset_min: -180 });
        r.start(t0, 1_709_210_096_789, &[], false);
        fly(&mut r, t0, 0.0, 8.0, 70);
        r.conditions(&[Condition::Wind], at(t0, 2.0));
        r.warning(WarningCode::Other, "<script>alert('x')</script> & cia", at(t0, 3.0));
        let report = r.finish(at(t0, 8.0), false).unwrap();
        let html = report.render_html();
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("29/02/2024, 09:34:56 (UTC−03:00)"), "hora local");
        assert_eq!(report.file_name(), "voo-20240229-093456.html");
        assert!(html.contains("<svg") && html.contains("<polyline") && html.contains("class=\"track\"><i style="));
        assert!(html.contains("Linha do tempo") && html.contains("Vento forte"));
        assert!(html.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; cia"));
        assert!(!html.contains("<script"), "nada de JS");
        assert!(!html.contains("http"), "sem recursos externos");
    }

    #[test]
    fn gaps_split_the_chart_line() {
        let mut out = String::new();
        let pts = [(0.0, Some(1.0)), (1.0, Some(2.0)), (2.0, None), (3.0, Some(3.0))];
        line_chart(&mut out, "X", "", 3.0, &pts, (0.0, 1.0));
        assert_eq!(out.matches("<polyline").count(), 2);
        let mut empty = String::new();
        line_chart(&mut empty, "X", "", 3.0, &[(0.0, None)], (0.0, 1.0));
        assert!(empty.is_empty());
    }

    #[test]
    fn formats_numbers_in_portuguese() {
        assert_eq!(dec(1.26, 1), "1,3");
        assert_eq!(pct(3.456), "3,5%");
        assert_eq!(pct(12.4), "12%");
        assert_eq!(duration(59), "59 s");
        assert_eq!(duration(120), "2 min");
        assert_eq!(duration(125), "2 min 5 s");
        assert_eq!(offset_label(330), "UTC+05:30");
        assert_eq!(offset_label(0), "UTC");
    }
}
