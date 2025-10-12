use std::fmt::{Display, Formatter};
use std::io;
use t2v_module::NUsbError;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Usb(NUsbError),
    Join(tokio::task::JoinError),
}

pub type Result<T> = ::std::result::Result<T, Error>;

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Usb(e) => write!(f, "usb error: {e}"),
            Error::Join(e) => write!(f, "join error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<NUsbError> for Error {
    fn from(e: NUsbError) -> Self {
        Error::Usb(e)
    }
}

impl From<tokio::task::JoinError> for Error {
    fn from(e: tokio::task::JoinError) -> Self {
        Error::Join(e)
    }
}
