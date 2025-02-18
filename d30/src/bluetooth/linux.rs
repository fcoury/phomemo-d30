#[cfg(target_os = "linux")]
use advmac::MacAddr6;
#[cfg(target_os = "linux")]
use bluetooth_serial_port_async::{BtAddr, BtProtocol, BtSocket as LinuxBtSocket};

#[cfg(target_os = "linux")]
pub struct LinuxBluetoothSocket {
    socket: LinuxBtSocket,
}

#[cfg(target_os = "linux")]
impl LinuxBluetoothSocket {
    pub fn new() -> Result<Self, BluetoothError> {
        Ok(Self {
            socket: LinuxBtSocket::new(BtProtocol::RFCOMM)
                .map_err(|e| BluetoothError::ConnectionFailed(e.to_string()))?,
        })
    }
}

#[cfg(target_os = "linux")]
impl BluetoothSocket for LinuxBluetoothSocket {
    fn connect(&mut self, addr: &str) -> Result<(), BluetoothError> {
        // Parse the MAC address string to MacAddr6
        let mac_addr = addr
            .parse::<MacAddr6>()
            .map_err(|e| BluetoothError::ConnectionFailed(format!("Invalid MAC address: {}", e)))?;

        // Convert to BtAddr format and connect
        self.socket
            .connect(BtAddr(mac_addr.to_array()))
            .map_err(|e| BluetoothError::ConnectionFailed(e.to_string()))?;

        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> Result<(), BluetoothError> {
        self.socket
            .write(data)
            .map_err(|e| BluetoothError::WriteError(e.to_string()))?;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), BluetoothError> {
        self.socket
            .flush()
            .map_err(|e| BluetoothError::WriteError(e.to_string()))?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl Drop for LinuxBluetoothSocket {
    fn drop(&mut self) {
        // Socket is automatically closed when dropped
    }
}
