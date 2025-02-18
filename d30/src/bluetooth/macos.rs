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

// UUID for Serial Port Profile
const SPP_UUID: &str = "00001101-0000-1000-8000-00805F9B34FB";
// UUID for Serial Port Profile Service Class
const SPP_SERVICE_CLASS_UUID: &str = "00001101-0000-1000-8000-00805F9B34FB";

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
                        "IOBluetooth framework not found".to_string(),
                    ));
                }
            };

            // Get the device
            self.device = msg_send![device_class, withAddressString:addr_str];
            if self.device == nil {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::ConnectionFailed(
                    "Device not found".to_string(),
                ));
            }

            // Force fresh service discovery
            debug!("Performing service discovery...");
            let _: bool = msg_send![self.device, performSDPQuery:nil];
            thread::sleep(Duration::from_millis(1000));

            // Connect if not already connected
            let is_connected: bool = msg_send![self.device, isConnected];
            if !is_connected {
                debug!("Device not connected, attempting to connect...");
                let connect_result: bool = msg_send![self.device, openConnection];
                if !connect_result {
                    let _: () = msg_send![pool, release];
                    return Err(BluetoothError::ConnectionFailed(
                        "Failed to open connection".to_string(),
                    ));
                }
                thread::sleep(Duration::from_millis(1000));
            }

            // Get services
            debug!("Looking for Serial Port Profile service...");

            // Get the CBUUID class and create UUID object
            let uuid_class = match Class::get("CBUUID") {
                Some(class) => class,
                None => {
                    let _: () = msg_send![pool, release];
                    return Err(BluetoothError::InternalError(
                        "CBUUID class not found".to_string(),
                    ));
                }
            };

            let spp_uuid = NSString::alloc(nil).init_str(SPP_UUID);
            let uuid_obj: id = msg_send![uuid_class, UUIDWithString:spp_uuid];

            // Get service with SPP UUID
            let service: id = msg_send![self.device, getServiceRecordForUUID:uuid_obj];
            if service == nil {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::ConnectionFailed(
                    "Serial Port Profile service not found".to_string(),
                ));
            }

            // Get RFCOMM channel from service
            let mut channel_id: u8 = 0;
            let result: bool = msg_send![service, getRFCOMMChannelID:&mut channel_id];
            if !result {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::ConnectionFailed(
                    "Could not get RFCOMM channel ID".to_string(),
                ));
            }

            debug!("Found RFCOMM channel ID: {}", channel_id);

            // Open the RFCOMM channel
            let mut rfcomm_channel: id = nil;
            let result: bool = msg_send![self.device,
                                       openRFCOMMChannelSync:&mut rfcomm_channel
                                       withChannelID:channel_id
                                       delegate:nil];

            if !result || rfcomm_channel == nil {
                let _: () = msg_send![pool, release];
                return Err(BluetoothError::ConnectionFailed(format!(
                    "Failed to open RFCOMM channel {}",
                    channel_id
                )));
            }

            self.channel = rfcomm_channel;

            let _: () = msg_send![pool, release];
            debug!("Successfully connected to device on channel {}", channel_id);
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
