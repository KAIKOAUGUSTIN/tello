use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// O drone não respondeu dentro do prazo.
    Timeout(String),
    /// O drone respondeu, mas recusou o comando (`error ...`).
    Refused(String),
    /// O recurso não existe no protocolo ou modelo atual.
    NotSupported(&'static str),
    NotConnected,
    Protocol(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "erro de rede: {e}"),
            Error::Timeout(what) => write!(f, "sem resposta do drone: {what}"),
            Error::Refused(msg) => write!(f, "o drone recusou o comando: {msg}"),
            Error::NotSupported(what) => write!(f, "não suportado neste modo/modelo: {what}"),
            Error::NotConnected => write!(f, "não conectado"),
            Error::Protocol(msg) => write!(f, "erro de protocolo: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
