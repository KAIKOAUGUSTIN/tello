//! Utilidades de rede: sockets UDP e diagnóstico da conexão Wi-Fi.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

use crate::Error;

/// Abre um socket UDP local, com mensagem clara quando a porta está ocupada.
pub fn bind_udp(port: u16, what: &str) -> Result<UdpSocket, Error> {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    UdpSocket::bind(addr).map_err(|e| {
        if e.kind() == io::ErrorKind::AddrInUse {
            Error::Protocol(format!(
                "a porta UDP {port} ({what}) já está em uso. Feche o app oficial do Tello \
                 ou outro programa que esteja conectado ao drone."
            ))
        } else {
            Error::Io(e)
        }
    })
}

pub(crate) fn set_poll_timeout(sock: &UdpSocket) -> io::Result<()> {
    sock.set_read_timeout(Some(Duration::from_millis(200)))
}

pub(crate) fn is_timeout(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
}

/// IP local que o sistema usaria para falar com `target`, sem enviar nada.
pub fn local_ip_towards(target: IpAddr) -> Option<IpAddr> {
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    sock.connect((target, 8889)).ok()?;
    sock.local_addr().ok().map(|a| a.ip())
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct NetworkCheck {
    pub local_ip: Option<IpAddr>,
    pub same_subnet: bool,
}

/// Verifica se o computador está na mesma rede /24 do drone (Wi-Fi TELLO-XXXXXX).
pub fn check_network(drone_ip: IpAddr) -> NetworkCheck {
    let local_ip = local_ip_towards(drone_ip);
    let same_subnet = match (local_ip, drone_ip) {
        (Some(IpAddr::V4(a)), IpAddr::V4(b)) => a.is_loopback() || a.octets()[..3] == b.octets()[..3],
        _ => false,
    };
    NetworkCheck { local_ip, same_subnet }
}
