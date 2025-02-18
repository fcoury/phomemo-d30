use cocoa_foundation::{
    base::{id, nil},
    foundation::NSString,
};
use log::debug;
use objc::runtime::Class;
use objc::{class, msg_send, sel, sel_impl};
use std::thread;
use std::time::Duration;

use super::BluetoothError;
use super::BluetoothSocket;

#[link(name = "IOBluetooth", kind = "framework")]
extern "C" {}

pub struct MacOSBluetoothSocket {
    device: id,
    channel: id,
}

impl BluetoothSocket for MacOSBluetoothSocket {
    fn connect(&mut self, addr: &str) -> Result<(), BluetoothError> {
        unsafe {
            debug!("Connecting to device with address: {}", addr);
            let pool: id = msg_send![class!(NSAutoreleasePool), new];

            // Format MAC address with hyphens for IOBluetooth
            let addr = addr.replace(":", "-");
            let addr_str = NSString::alloc(nil).init_str(&addr);

            let device_class = match Class::get("IOBluetoothDevice") {
                Some(class) => class,
                None => {
                    let _: () = msg_send![pool, release];
                    return Err(BluetoothError::InternalError(
                        "IOBluetooth framework not found. Make sure Bluetooth is enabled."
                            .to_string(),
                    ));
                }
            };

            // First try to get paired device
            self.device = msg_send![device_class, withAddressString:addr_str];
            if self.device == nil {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::ConnectionFailed(
                    "Device not found. Make sure it's paired in System Settings.".to_string(),
                ));
            }

            // Force an SDP query first
            debug!("Performing SDP query...");
            let _: bool = msg_send![self.device, performSDPQuery:nil];
            thread::sleep(Duration::from_millis(1000));

            // Check if already connected
            let is_connected: bool = msg_send![self.device, isConnected];
            if !is_connected {
                debug!("Device not connected, attempting to connect...");

                // Try to connect multiple times
                for attempt in 1..=3 {
                    debug!("Connection attempt {} of 3", attempt);
                    let connect_result: bool = msg_send![self.device, openConnection];
                    if connect_result {
                        // Wait for connection to establish
                        for i in 0..10 {
                            let is_connected: bool = msg_send![self.device, isConnected];
                            if is_connected {
                                debug!("Connection established after {} checks", i + 1);
                                break;
                            }
                            thread::sleep(Duration::from_millis(500));
                        }

                        let is_connected: bool = msg_send![self.device, isConnected];
                        if is_connected {
                            break;
                        }
                    }
                    thread::sleep(Duration::from_millis(1000));
                }

                let is_connected: bool = msg_send![self.device, isConnected];
                if !is_connected {
                    let _: () = msg_send![pool, release];
                    return Err(BluetoothError::ConnectionFailed(
                        "Failed to establish connection after multiple attempts".to_string(),
                    ));
                }
            }

            debug!("Device connected, discovering services...");

            // Look for Serial Port Profile service
            let mut services: id = msg_send![self.device, services];
            if services == nil {
                debug!("No services found, performing another SDP query...");
                let _: bool = msg_send![self.device, performSDPQuery:nil];
                thread::sleep(Duration::from_millis(1000));
                services = msg_send![self.device, services];
            }

            if services == nil {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::ConnectionFailed(
                    "No services found on device".to_string(),
                ));
            }

            let count: usize = msg_send![services, count];
            debug!("Found {} services", count);

            // Try channels in order of probability for this device
            let channel_ids = [4u8, 1u8, 2u8, 3u8]; // Starting with 4 as it's common for printers
            let mut success = false;

            for &channel_id in &channel_ids {
                debug!("Trying RFCOMM channel {}", channel_id);
                let mut rfcomm_channel: id = nil;

                // Attempt to open channel multiple times
                for attempt in 1..=3 {
                    debug!("Channel {} attempt {} of 3", channel_id, attempt);
                    let result: bool = msg_send![self.device,
                                               openRFCOMMChannelSync:&mut rfcomm_channel
                                               withChannelID:channel_id
                                               delegate:nil];

                    if result && rfcomm_channel != nil {
                        self.channel = rfcomm_channel;
                        success = true;
                        debug!("Successfully opened RFCOMM channel {}", channel_id);
                        break;
                    }
                    thread::sleep(Duration::from_millis(200));
                }

                if success {
                    break;
                }
            }

            if !success {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::ConnectionFailed(
                    "Failed to open RFCOMM channel after multiple attempts".to_string(),
                ));
            }

            let _: () = msg_send![pool, release];
            debug!("Successfully connected to device");
            Ok(())
        }
    }

    fn write(&mut self, data: &[u8]) -> Result<(), BluetoothError> {
        unsafe {
            let pool: id = msg_send![class!(NSAutoreleasePool), new];

            if self.channel == nil {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::NotConnected);
            }

            debug!("Writing {} bytes to device", data.len());

            // Try to get MTU, default to 128 if not available
            let mtu: u16 = msg_send![self.channel, getMTU];
            let chunk_size = if mtu > 0 { mtu as usize } else { 128 };
            debug!("Using chunk size of {} bytes", chunk_size);

            // Write data in chunks
            for (i, chunk) in data.chunks(chunk_size).enumerate() {
                debug!("Writing chunk {} ({} bytes)", i + 1, chunk.len());
                let result: bool = msg_send![self.channel,
                                           writeSync:chunk
                                           length:chunk.len()];

                if !result {
                    let _: () = msg_send![pool, release];
                    return Err(BluetoothError::WriteError(format!(
                        "Failed to write chunk {} to device",
                        i + 1
                    )));
                }
                thread::sleep(Duration::from_millis(50));
            }

            let _: () = msg_send![pool, release];
            debug!("Successfully wrote all data");
            Ok(())
        }
    }

    fn flush(&mut self) -> Result<(), BluetoothError> {
        Ok(())
    }
}

impl MacOSBluetoothSocket {
    pub fn new() -> Result<Self, BluetoothError> {
        Ok(MacOSBluetoothSocket {
            device: nil,
            channel: nil,
        })
    }
}

impl Drop for MacOSBluetoothSocket {
    fn drop(&mut self) {
        unsafe {
            if self.channel != nil {
                let pool: id = msg_send![class!(NSAutoreleasePool), new];

                debug!("Closing RFCOMM channel");
                let _: () = msg_send![self.channel, closeChannel];

                if self.device != nil {
                    debug!("Closing connection to device");
                    let _: () = msg_send![self.device, closeConnection];
                }

                let _: () = msg_send![pool, release];
            }
        }
    }
}
