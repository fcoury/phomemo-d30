use std::error::Error as StdError;
use std::fmt;

#[derive(Debug)]
pub enum BluetoothError {
    ConnectionFailed(String),
    WriteError(String),
    NotConnected,
    InternalError(String),
}

impl fmt::Display for BluetoothError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BluetoothError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            BluetoothError::WriteError(msg) => write!(f, "Write error: {}", msg),
            BluetoothError::NotConnected => write!(f, "Not connected to device"),
            BluetoothError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl StdError for BluetoothError {}

pub trait BluetoothSocket {
    fn connect(&mut self, addr: &str) -> Result<(), BluetoothError>;
    fn write(&mut self, data: &[u8]) -> Result<(), BluetoothError>;
    fn flush(&mut self) -> Result<(), BluetoothError>;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::LinuxBluetoothSocket;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::MacOSBluetoothSocket;

#[cfg(target_os = "linux")]
pub fn create_socket() -> Result<impl BluetoothSocket, BluetoothError> {
    LinuxBluetoothSocket::new()
}

#[cfg(target_os = "macos")]
pub fn create_socket() -> Result<impl BluetoothSocket, BluetoothError> {
    MacOSBluetoothSocket::new()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn create_socket() -> Result<impl BluetoothSocket, BluetoothError> {
    Err(BluetoothError::InternalError(
        "Unsupported platform".to_string(),
    ))
}
