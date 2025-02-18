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

            // Format MAC address for IOBluetooth
            let addr = addr.replace(":", "-");
            let addr_str = NSString::alloc(nil).init_str(&addr);
            debug!("Using formatted address: {}", addr);

            let device_class = match Class::get("IOBluetoothDevice") {
                Some(class) => class,
                None => {
                    let _: () = msg_send![pool, release];
                    return Err(BluetoothError::InternalError(
                        "IOBluetooth framework not found".to_string(),
                    ));
                }
            };

            // Get device
            self.device = msg_send![device_class, deviceWithAddressString:addr_str];
            if self.device == nil {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::ConnectionFailed(
                    "Device not found".to_string(),
                ));
            }

            debug!("Got device, checking name...");
            let name: id = msg_send![self.device, name];
            if name != nil {
                let name_str: id = msg_send![name, UTF8String];
                debug!("Device name: {:?}", name_str);
            }

            // Force service discovery
            debug!("Performing service discovery...");
            let inquiry_result: bool = msg_send![self.device, performSDPQuery:nil];
            debug!("SDP query result: {}", inquiry_result);
            thread::sleep(Duration::from_millis(1000));

            // Check device properties
            let is_paired: bool = msg_send![self.device, isPaired];
            let is_connected: bool = msg_send![self.device, isConnected];
            debug!(
                "Device status - Paired: {}, Connected: {}",
                is_paired, is_connected
            );

            // Try to connect if not already connected
            if !is_connected {
                debug!("Attempting to connect...");
                let connect_result: bool = msg_send![self.device, openConnection];
                if !connect_result {
                    let _: () = msg_send![pool, release];
                    return Err(BluetoothError::ConnectionFailed(
                        "Failed to open connection".to_string(),
                    ));
                }
                thread::sleep(Duration::from_millis(1000));
            }

            // Get all services
            let mut services: id = msg_send![self.device, services];
            if services == nil {
                debug!("No services found, trying SDP query again...");
                let _: bool = msg_send![self.device, performSDPQuery:nil];
                thread::sleep(Duration::from_millis(1000));
                services = msg_send![self.device, services];
            }

            if services == nil {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::ConnectionFailed(
                    "No services available".to_string(),
                ));
            }

            let count: usize = msg_send![services, count];
            debug!("Found {} services", count);

            // Inspect each service
            for i in 0..count {
                let service: id = msg_send![services, objectAtIndex:i];
                if service != nil {
                    // Try to get service UUID
                    let uuid: id = msg_send![service, getServiceUUID];
                    if uuid != nil {
                        let uuid_str: id = msg_send![uuid, UUIDString];
                        debug!("Service {}: UUID = {:?}", i, uuid_str);
                    }

                    // Try to get RFCOMM channel
                    let mut channel_id: u8 = 0;
                    let has_channel: bool = msg_send![service, getRFCOMMChannelID:&mut channel_id];
                    if has_channel {
                        debug!("Service {} has RFCOMM channel: {}", i, channel_id);

                        // Try to open this channel
                        let mut rfcomm_channel: id = nil;
                        debug!("Attempting to open channel {}...", channel_id);
                        let result: bool = msg_send![self.device,
                                                   openRFCOMMChannelSync:&mut rfcomm_channel
                                                   withChannelID:channel_id
                                                   delegate:nil];

                        if result && rfcomm_channel != nil {
                            debug!("Successfully opened channel {}", channel_id);
                            self.channel = rfcomm_channel;
                            let _: () = msg_send![pool, release];
                            return Ok(());
                        } else {
                            debug!("Failed to open channel {}", channel_id);
                        }
                    }
                }
            }

            let _: () = msg_send![pool, release];
            Err(BluetoothError::ConnectionFailed(
                "Could not find usable RFCOMM channel".to_string(),
            ))
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

            // Use small chunks to avoid buffer issues
            const CHUNK_SIZE: usize = 64;
            for (i, chunk) in data.chunks(CHUNK_SIZE).enumerate() {
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
