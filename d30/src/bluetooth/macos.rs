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
    peripheral: id,
    channel: id,
}

impl BluetoothSocket for MacOSBluetoothSocket {
    fn connect(&mut self, addr: &str) -> Result<(), BluetoothError> {
        unsafe {
            debug!("Connecting to device with address: {}", addr);
            let pool: id = msg_send![class!(NSAutoreleasePool), new];

            // Try to find the Bluetooth device
            let addr_str = NSString::alloc(nil).init_str(addr);
            self.peripheral = match Class::get("IOBluetoothDevice") {
                Some(class) => msg_send![class, withAddressString:addr_str],
                None => {
                    let _: () = msg_send![pool, release];
                    return Err(BluetoothError::InternalError(
                        "IOBluetooth framework not found. Make sure Bluetooth is enabled."
                            .to_string(),
                    ));
                }
            };

            if self.peripheral == nil {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::ConnectionFailed(
                    "Device not found. Make sure it's powered on and in range.".to_string(),
                ));
            }

            // Check if device is connected
            let is_connected: bool = msg_send![self.peripheral, isConnected];
            if !is_connected {
                debug!("Device not connected, attempting to connect...");
                let result: bool = msg_send![self.peripheral, openConnection];
                if !result {
                    let _: () = msg_send![pool, release];
                    return Err(BluetoothError::ConnectionFailed(
                        "Failed to connect to device".to_string(),
                    ));
                }
                // Wait for connection to establish
                thread::sleep(Duration::from_millis(500));
            }

            debug!("Opening RFCOMM channel...");
            let mut rfcomm_channel: id = nil;
            let channel_id: u8 = 1; // Standard SPP channel
            let result: bool = msg_send![self.peripheral,
                                       openRFCOMMChannelSync:&mut rfcomm_channel
                                       withChannelID:channel_id
                                       delegate:nil];

            if !result || rfcomm_channel == nil {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::ConnectionFailed(
                    "Failed to open RFCOMM channel".to_string(),
                ));
            }

            self.channel = rfcomm_channel;
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
            let result: bool = msg_send![self.channel,
                                       writeSync:data
                                       length:data.len()];

            let _: () = msg_send![pool, release];

            if !result {
                return Err(BluetoothError::WriteError(
                    "Failed to write data to device".to_string(),
                ));
            }

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
            peripheral: nil,
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

                if self.peripheral != nil {
                    debug!("Closing connection to device");
                    let _: () = msg_send![self.peripheral, closeConnection];
                }

                let _: () = msg_send![pool, release];
            }
        }
    }
}
