use log::debug;
use std::io::Write;
use std::process::Command;
use std::thread;
use std::time::Duration;

use super::BluetoothError;
use super::BluetoothSocket;

pub struct MacOSBluetoothSocket {
    device_addr: String,
}

impl BluetoothSocket for MacOSBluetoothSocket {
    fn connect(&mut self, addr: &str) -> Result<(), BluetoothError> {
        // First ensure blueutil is installed
        let which_output = Command::new("which")
            .arg("blueutil")
            .output()
            .map_err(|_| {
                BluetoothError::InternalError(
                    "blueutil not found. Install it with 'brew install blueutil'".to_string(),
                )
            })?;

        if !which_output.status.success() {
            return Err(BluetoothError::InternalError(
                "blueutil not found. Install it with 'brew install blueutil'".to_string(),
            ));
        }

        debug!("Found blueutil");

        // Make sure Bluetooth is on
        let power_output = Command::new("blueutil")
            .arg("--power")
            .output()
            .map_err(|e| {
                BluetoothError::InternalError(format!("Failed to check Bluetooth power: {}", e))
            })?;

        if power_output.stdout[0] != b'1' {
            debug!("Bluetooth is off, turning it on...");
            Command::new("blueutil")
                .args(["--power", "1"])
                .output()
                .map_err(|e| {
                    BluetoothError::InternalError(format!("Failed to turn on Bluetooth: {}", e))
                })?;

            // Wait for Bluetooth to initialize
            thread::sleep(Duration::from_secs(2));
        }

        // Format the address
        let addr = addr.replace("-", ":").to_uppercase();
        debug!("Using address: {}", addr);
        self.device_addr = addr.clone();

        // Check if device is paired
        let paired_output = Command::new("blueutil")
            .args(["--paired"])
            .output()
            .map_err(|e| {
                BluetoothError::InternalError(format!("Failed to check paired devices: {}", e))
            })?;

        let paired_str = String::from_utf8_lossy(&paired_output.stdout);
        if !paired_str.contains(&addr) {
            debug!("Device not paired, attempting to pair...");

            // Start discovery
            Command::new("blueutil")
                .args(["--inquiry"])
                .output()
                .map_err(|e| {
                    BluetoothError::InternalError(format!(
                        "Failed to start device discovery: {}",
                        e
                    ))
                })?;

            // Wait for discovery
            thread::sleep(Duration::from_secs(5));

            // Try to pair
            let pair_output = Command::new("blueutil")
                .args(["--pair", &addr])
                .output()
                .map_err(|e| {
                    BluetoothError::ConnectionFailed(format!("Failed to pair with device: {}", e))
                })?;

            if !pair_output.status.success() {
                return Err(BluetoothError::ConnectionFailed(
                    "Failed to pair with device".to_string(),
                ));
            }

            debug!("Successfully paired with device");
            thread::sleep(Duration::from_secs(1));
        }

        // Connect to device
        debug!("Connecting to device...");
        let connect_output = Command::new("blueutil")
            .args(["--connect", &addr])
            .output()
            .map_err(|e| {
                BluetoothError::ConnectionFailed(format!("Failed to connect to device: {}", e))
            })?;

        if !connect_output.status.success() {
            return Err(BluetoothError::ConnectionFailed(
                "Failed to connect to device".to_string(),
            ));
        }

        // Verify connection
        thread::sleep(Duration::from_secs(1));
        let info_output = Command::new("blueutil")
            .args(["--info", &addr])
            .output()
            .map_err(|e| {
                BluetoothError::InternalError(format!("Failed to get device info: {}", e))
            })?;

        let info_str = String::from_utf8_lossy(&info_output.stdout);
        if !info_str.contains("connected: 1") {
            return Err(BluetoothError::ConnectionFailed(
                "Device connection verification failed".to_string(),
            ));
        }

        debug!("Successfully connected to device");
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> Result<(), BluetoothError> {
        debug!("Writing {} bytes to device", data.len());

        // Use hcitool to write the data
        let mut child = Command::new("rfcomm")
            .args(["send", &self.device_addr])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| BluetoothError::WriteError(format!("Failed to start rfcomm: {}", e)))?;

        // Write data to stdin
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| BluetoothError::WriteError("Failed to open rfcomm stdin".to_string()))?;

        stdin
            .write_all(data)
            .map_err(|e| BluetoothError::WriteError(format!("Failed to write data: {}", e)))?;

        drop(stdin); // Close stdin so rfcomm knows we're done

        // Wait for process to complete
        let status = child
            .wait()
            .map_err(|e| BluetoothError::WriteError(format!("Failed to complete write: {}", e)))?;

        if !status.success() {
            return Err(BluetoothError::WriteError(
                "Failed to write data to device".to_string(),
            ));
        }

        debug!("Successfully wrote data to device");
        Ok(())
    }

    fn flush(&mut self) -> Result<(), BluetoothError> {
        Ok(())
    }
}

impl MacOSBluetoothSocket {
    pub fn new() -> Result<Self, BluetoothError> {
        Ok(MacOSBluetoothSocket {
            device_addr: String::new(),
        })
    }
}

impl Drop for MacOSBluetoothSocket {
    fn drop(&mut self) {
        if !self.device_addr.is_empty() {
            debug!("Disconnecting from device");
            if let Ok(output) = Command::new("blueutil")
                .args(["--disconnect", &self.device_addr])
                .output()
            {
                if output.status.success() {
                    debug!("Successfully disconnected from device");
                }
            }
        }
    }
}
