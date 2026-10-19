use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tello_sim::{Sim, SimConfig};

const HELP: &str = "\
tello-sim — simulador de Tello para desenvolvimento

USO: tello-sim [opções]

  --sdk <1.3|2.0|3.0>   versão do SDK simulada (padrão 2.0)
  --battery <0-100>     bateria inicial (padrão 87)
  --port <n>            porta de comandos (padrão 8889)
  --state-port <n>      porta do cliente para o estado (padrão 8890)
  --video-port <n>      porta do cliente para o vídeo (padrão 11111)
  --public              aceita conexões da rede (padrão: só 127.0.0.1)

No cliente, use o IP 127.0.0.1 para conectar ao simulador.";

fn main() {
    let mut cfg = SimConfig::default();
    let mut ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut port = 8889;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| fail(&format!("falta o valor de {arg}")));
        match arg.as_str() {
            "--sdk" => {
                cfg.sdk_version = match value().as_str() {
                    "1.3" => None,
                    "2.0" => Some("20".into()),
                    "3.0" => Some("30".into()),
                    other => fail(&format!("versão de SDK inválida: {other}")),
                }
            }
            "--battery" => cfg.battery = parse(&value()),
            "--port" => port = parse(&value()),
            "--state-port" => cfg.state_port = parse(&value()),
            "--video-port" => cfg.video_port = parse(&value()),
            "--public" => ip = IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            "-h" | "--help" => {
                println!("{HELP}");
                return;
            }
            other => fail(&format!("opção desconhecida: {other}")),
        }
    }
    cfg.bind = SocketAddr::new(ip, port);
    let sdk = cfg.sdk_version.clone().unwrap_or_else(|| "1.3".into());
    let sim = Sim::start_verbose(cfg).unwrap_or_else(|e| fail(&format!("não foi possível abrir a porta: {e}")));
    println!("Simulador ouvindo em {} (SDK {sdk}). Ctrl+C para sair.", sim.addr());
    loop {
        std::thread::park();
    }
}

fn parse<T: std::str::FromStr>(s: &str) -> T {
    s.parse().unwrap_or_else(|_| fail(&format!("valor inválido: {s}")))
}

fn fail(msg: &str) -> ! {
    eprintln!("erro: {msg}\n\n{HELP}");
    std::process::exit(2)
}
