//! `tello-cli`: diagnóstico da conexão com o drone (sem decolar) e um console
//! para enviar comandos do SDK manualmente.

use std::fs::File;
use std::io::{self, BufRead, BufWriter, IsTerminal, Write};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tello_core::net::check_network;
use tello_core::video::{find_sps, parse_sps};
use tello_core::{Action, Config, Drone, DroneEvent, Protocol, Telemetry};

const HELP: &str = "\
tello-cli — diagnóstico do Tello

USO:
  tello-cli [diag] [opções]   testa a conexão sem decolar (padrão)
  tello-cli shell [opções]    console para enviar comandos do SDK

OPÇÕES:
  --ip <ip>            IP do drone (padrão 192.168.10.1; simulador: 127.0.0.1)
  --segundos <n>       duração de cada etapa do diagnóstico (padrão 10)
  --saida <pasta>      onde salvar vídeo e log (padrão: pasta atual)
  --sem-pausa          não espera Enter no final";

struct Options {
    command: String,
    ip: Option<IpAddr>,
    seconds: u64,
    out_dir: PathBuf,
    pause: bool,
}

fn main() {
    let opts = parse_args();
    let ok = match opts.command.as_str() {
        "diag" => diag(&opts),
        "shell" => shell(&opts),
        _ => unreachable!(),
    };
    if opts.pause && io::stdin().is_terminal() {
        print!("\nPressione Enter para sair...");
        let _ = io::stdout().flush();
        let _ = io::stdin().lock().read_line(&mut String::new());
    }
    std::process::exit(if ok { 0 } else { 1 });
}

fn parse_args() -> Options {
    let mut opts = Options { command: "diag".into(), ip: None, seconds: 10, out_dir: PathBuf::from("."), pause: true };
    let mut args = std::env::args().skip(1).peekable();
    if let Some(first) = args.peek() {
        if first == "diag" || first == "shell" {
            opts.command = args.next().unwrap();
        }
    }
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| fail(&format!("falta o valor de {arg}")));
        match arg.as_str() {
            "--ip" => opts.ip = Some(value().parse().unwrap_or_else(|_| fail("IP inválido"))),
            "--segundos" => opts.seconds = value().parse().unwrap_or_else(|_| fail("número inválido")),
            "--saida" => opts.out_dir = PathBuf::from(value()),
            "--sem-pausa" => opts.pause = false,
            "-h" | "--help" => {
                println!("{HELP}");
                std::process::exit(0);
            }
            other => fail(&format!("opção desconhecida: {other}")),
        }
    }
    opts
}

fn fail(msg: &str) -> ! {
    eprintln!("erro: {msg}\n\n{HELP}");
    std::process::exit(2)
}

fn config(opts: &Options) -> Config {
    let mut cfg = Config::default();
    if let Some(ip) = opts.ip {
        cfg.drone_ip = ip;
    }
    cfg
}

#[derive(Default)]
struct Collected {
    telemetry: Option<Telemetry>,
    warnings: Vec<String>,
    video: Option<BufWriter<File>>,
    sps: Option<String>,
}

fn diag(opts: &Options) -> bool {
    let cfg = config(opts);
    let stamp = timestamp();
    let secs = opts.seconds.max(1);
    println!("== Diagnóstico do Tello (não decola) ==\n");

    // 1. Rede
    let net = check_network(cfg.drone_ip);
    let wifi_ok = net.same_subnet;
    match net.local_ip {
        Some(ip) if wifi_ok => println!("[ok]   Rede: IP local {ip}, mesma rede do drone {}", cfg.drone_ip),
        Some(ip) => println!(
            "[aviso] Rede: IP local {ip} não está na rede do drone ({}). Conecte-se ao Wi-Fi TELLO-XXXXXX.",
            cfg.drone_ip
        ),
        None => println!("[aviso] Rede: não há rota para {}. O Wi-Fi está ligado?", cfg.drone_ip),
    }

    // 2. Conexão
    let state_packets = Arc::new(AtomicUsize::new(0));
    let frames = Arc::new(AtomicUsize::new(0));
    let keyframes = Arc::new(AtomicUsize::new(0));
    let collected = Arc::new(Mutex::new(Collected::default()));
    let sink = {
        let (state_packets, frames, keyframes, collected) =
            (state_packets.clone(), frames.clone(), keyframes.clone(), collected.clone());
        Arc::new(move |e: DroneEvent| match e {
            DroneEvent::Telemetry { data } => {
                state_packets.fetch_add(1, Ordering::Relaxed);
                collected.lock().unwrap().telemetry = Some(data);
            }
            DroneEvent::Warning { message, .. } => collected.lock().unwrap().warnings.push(message),
            DroneEvent::Video(f) => {
                frames.fetch_add(1, Ordering::Relaxed);
                keyframes.fetch_add(f.keyframe as usize, Ordering::Relaxed);
                let mut c = collected.lock().unwrap();
                if c.sps.is_none() {
                    if let Some(info) = find_sps(&f.data).and_then(parse_sps) {
                        c.sps = Some(format!(
                            "H.264 {} nível {:.1}, {}x{} ({})",
                            info.profile_name(),
                            info.level_idc as f32 / 10.0,
                            info.width,
                            info.height,
                            info.codec_string()
                        ));
                    }
                }
                if let Some(w) = c.video.as_mut() {
                    let _ = w.write_all(&f.data);
                }
            }
            _ => {}
        })
    };

    print!("[...]  Conectando ao drone em {} ... ", cfg.drone_ip);
    let _ = io::stdout().flush();
    let drone = match Drone::connect(cfg, Protocol::Sdk, sink) {
        Ok(d) => {
            println!("ok");
            d
        }
        Err(e) => {
            println!("falhou\n[erro]  {e}");
            return false;
        }
    };
    let caps = drone.caps();
    println!(
        "[ok]   Comandos: modelo {:?}, SDK {}, serial {}",
        caps.model,
        caps.sdk_version.as_deref().unwrap_or("?"),
        caps.serial.as_deref().unwrap_or("?")
    );

    // 3. Telemetria
    println!("\n-- Telemetria por {secs} s --");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        sleep(Duration::from_secs(1));
        match collected.lock().unwrap().telemetry.clone() {
            Some(t) => println!(
                "  bateria {}%  altura {} cm  ToF {} cm  temp {}-{} °C  pitch/roll/yaw {}/{}/{}",
                show(t.battery),
                show(t.height_cm),
                show(t.tof_cm),
                show(t.temp_low_c),
                show(t.temp_high_c),
                show(t.pitch_deg),
                show(t.roll_deg),
                show(t.yaw_deg)
            ),
            None => println!("  (nenhum pacote de estado ainda)"),
        }
    }
    let states = state_packets.load(Ordering::Relaxed);
    // A conexão já gera uma telemetria com bateria/wifi vinda das consultas.
    let state_ok = states > secs as usize * 3;

    // 4. Vídeo
    let video_path = opts.out_dir.join(format!("tello-video-{stamp}.h264"));
    println!("\n-- Vídeo por {secs} s --");
    match File::create(&video_path) {
        Ok(f) => collected.lock().unwrap().video = Some(BufWriter::new(f)),
        Err(e) => println!("  não foi possível criar {}: {e}", video_path.display()),
    }
    let stream = drone.action(&Action::StreamOn);
    if let Err(e) = &stream {
        println!("  streamon falhou: {e}");
    }
    sleep(Duration::from_secs(secs));
    let _ = drone.action(&Action::StreamOff);
    let mut c = collected.lock().unwrap();
    if let Some(mut w) = c.video.take() {
        let _ = w.flush();
    }
    let sps = c.sps.clone();
    let warnings = c.warnings.clone();
    drop(c);
    let frames = frames.load(Ordering::Relaxed);
    let keyframes = keyframes.load(Ordering::Relaxed);
    let video_ok = frames > 0;
    println!("  {frames} frames recebidos ({keyframes} chave). {}", sps.as_deref().unwrap_or(""));

    // 5. Log
    let log_path = opts.out_dir.join(format!("tello-log-{stamp}.jsonl"));
    let log_saved = File::create(&log_path).and_then(|f| drone.blackbox().export_jsonl(BufWriter::new(f)));
    drone.disconnect();

    // Resumo
    println!("\n== Resumo ==");
    line(wifi_ok, "Wi-Fi do Tello", "o PC não parece estar no Wi-Fi TELLO-XXXXXX");
    line(true, "Comandos (UDP 8889)", "");
    line(state_ok, "Telemetria (UDP 8890)", "bloqueada: libere o programa no Firewall do Windows para redes públicas");
    line(video_ok, "Vídeo (UDP 11111)", "não chegou: libere o programa no Firewall do Windows para redes públicas");
    for w in &warnings {
        println!("        aviso: {w}");
    }
    if video_ok {
        println!("\nVídeo salvo em {} (abra com o VLC).", video_path.display());
    } else {
        let _ = std::fs::remove_file(&video_path);
    }
    match log_saved {
        Ok(()) => println!("Log salvo em {}. Envie este arquivo se algo deu errado.", log_path.display()),
        Err(e) => println!("Não foi possível salvar o log: {e}"),
    }
    wifi_ok && state_ok && video_ok
}

fn shell(opts: &Options) -> bool {
    let cfg = config(opts);
    println!("Conectando em {} ...", cfg.drone_ip);
    let sink = Arc::new(|e: DroneEvent| {
        if let DroneEvent::Warning { message, .. } = e {
            println!("\n[aviso] {message}");
        }
    });
    let drone = match Drone::connect(cfg, Protocol::Sdk, sink) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("erro: {e}");
            return false;
        }
    };
    let caps = drone.caps();
    println!(
        "Conectado ({:?}, SDK {}). Digite comandos do SDK (ex.: battery?, takeoff, land).",
        caps.model,
        caps.sdk_version.as_deref().unwrap_or("?")
    );
    println!("CUIDADO: 'takeoff' faz o drone decolar de verdade. 'emergency' corta os motores. 'sair' encerra.\n");
    let stdin = io::stdin();
    loop {
        print!("tello> ");
        let _ = io::stdout().flush();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let text = line.trim();
        match text {
            "" => continue,
            "sair" | "exit" | "quit" => break,
            "emergency" => println!("{:?}", drone.action(&Action::Emergency)),
            _ => match drone.action(&Action::Raw { text: text.into() }) {
                Ok(r) => println!("{r}"),
                Err(e) => println!("erro: {e}"),
            },
        }
    }
    drone.disconnect();
    true
}

fn line(ok: bool, what: &str, hint: &str) {
    if ok {
        println!("[ok]   {what}");
    } else {
        println!("[falha] {what}: {hint}");
    }
}

fn show<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map(|v| v.to_string()).unwrap_or_else(|| "?".into())
}

/// Data/hora UTC no formato AAAAMMDD-HHMMSS, sem dependências.
fn timestamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Algoritmo "civil from days" (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}
