#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![forbid(clippy::unwrap_used)]
#![forbid(clippy::expect_used)]
#![warn(missing_docs)]

//! Asynchronous library to connect to the Roboracer Track to Vehicle (T2V) module.
//!
//! For details on how to build and set up the hardware required for receiving commands see:
//!
//! For details on how to send commands to your T2V module:
//!
//! Demo application to test your hardware:

use async_std::stream::Stream;
use futures_lite::StreamExt;
use log::debug;
use nusb::io::EndpointRead;
use nusb::transfer::{In, Interrupt};
use nusb::{Device, DeviceInfo, Interface};
use std::ops::Deref;
use tokio::io::AsyncReadExt;

use tokio::io;

pub use nusb::Error as NUsbError;
use nusb::hotplug::HotplugEvent;
use serde::{Deserialize, Serialize};

/// Initial state of the T2V module. At this stage the device has been initialized by the OS and is
/// available to be opened for communication.
///
/// ```no_run
/// # use std::error::Error;
/// use t2v_module::T2VModule;
///
/// # #[async_std::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// let devices = T2VModule::find_connected().await?;
/// let device = devices.next().expect("at least one device");
/// # }
/// ```
pub struct Initial {
    device_info: DeviceInfo,
}

/// State after the USB device has been opened.
///
/// ```no_run
/// # use std::error::Error;
/// use t2v_module::T2VModule;
///
/// # #[async_std::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// let devices = T2VModule::find_connected().await?;
/// let device = devices.next().expect("at least one device").open()?;
/// # }
/// ```
pub struct Opened {
    device: Device,
}

/// State of the device, after the USB interface has been claimed.
/// ```no_run
/// # use std::error::Error;
/// use t2v_module::T2VModule;
///
/// # #[async_std::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// let devices = T2VModule::find_connected().await?;
/// let device = devices.next().expect("at least one device").open()?.claim()?;
/// # }
/// ```
pub struct Claimed {
    device: Device,
    interface: Interface,
}

/// Main entry point for the T2V API of this library.
///
/// After constructing an (or many) instance of a T2V module, the endpoints exposed by the device are
/// exposed by type safe wrapper.
/// This allows to securely access information from the device.
pub struct T2VModule<State> {
    state: State,
}

impl T2VModule<Initial> {
    /// Find all connected devices on the system.
    ///
    /// This method only filters for connected devices with the vendor id `5455` and product id `1911`.
    /// For any other combinations, use [`find_connected_custom`](Self::find_connected_custom).
    ///
    /// You can further filter the devices by attributes available in [`DeviceInfo`](nusb::DeviceInfo).
    ///
    /// # Errors
    ///
    /// This method fails, if listing the USB devices fails. See [`list_devices`](nusb::list_devices) for more details.
    pub async fn find_connected() -> Result<impl Stream<Item = Self>, NUsbError> {
        Self::find_connected_custom(0x5455, 0x1911).await
    }

    /// Find all connected devices to the system with the given vendor and product id.
    ///
    /// You can further filter the devices by attributes available in [`DeviceInfo`](nusb::DeviceInfo).
    ///
    /// # Errors
    ///
    /// This method fails, if listing the USB devices fails. See [`list_devices`](nusb::list_devices) for more details.
    pub async fn find_connected_custom(
        vendor_id: u16,
        product_id: u16,
    ) -> Result<impl Stream<Item = Self>, NUsbError> {
        let device_watch = nusb::watch_devices()?;
        Ok(futures_lite::stream::iter(
            nusb::list_devices()
                .await?
                .filter(move |dev| dev.vendor_id() == vendor_id && dev.product_id() == product_id)
                .map(|dev| Self {
                    state: Initial { device_info: dev },
                }),
        )
        .chain(device_watch.filter_map(move |event| match event {
            HotplugEvent::Connected(device_info) => {
                if device_info.vendor_id() == vendor_id && device_info.product_id() == product_id {
                    debug!("Found matching device");
                    Some(Self {
                        state: Initial { device_info },
                    })
                } else {
                    None
                }
            }
            HotplugEvent::Disconnected(_) => None,
        })))
    }

    /// Opens the device.
    ///
    /// # Errors
    ///
    /// This method fails, if opening the device fails. See [`DeviceInfo::open`](nusb::DeviceInfo::open) for more details.
    pub async fn open(self) -> Result<T2VModule<Opened>, NUsbError> {
        let device = self.state.device_info.open().await?;
        Ok(T2VModule {
            state: Opened { device },
        })
    }
}

impl Deref for T2VModule<Initial> {
    type Target = DeviceInfo;

    fn deref(&self) -> &Self::Target {
        &self.state.device_info
    }
}

impl T2VModule<Opened> {
    /// Claim the required interface and detach the kernel driver to directly access the device.
    ///
    /// # Errors
    ///
    /// This method fails, if claiming the interface fails. See [`Device::detach_and_claim_interface`](nusb::Device::detach_and_claim_interface) for more details.
    pub async fn claim(self) -> Result<T2VModule<Claimed>, NUsbError> {
        let interface = self.state.device.detach_and_claim_interface(0).await?;
        Ok(T2VModule {
            state: Claimed {
                device: self.state.device,
                interface,
            },
        })
    }
}

impl T2VModule<Claimed> {
    /// Opens the endpoint dedicated to receiving IR NEC data wrapped in an instance of [`IrNecReader`](IrNecReader).
    ///
    /// # Errors
    ///
    /// This method fails, if opening the endpoint fails. See [`Interface::endpoint`](nusb::Interface::endpoint) for more details.
    pub fn ir_nec_endpoint(&self) -> Result<IrNecReader, NUsbError> {
        let endpoint = self.state.interface.endpoint::<Interrupt, In>(0x81)?;
        let endpoint_reader = endpoint.reader(4);
        Ok(IrNecReader {
            endpoint: endpoint_reader,
        })
    }

    /// Release the interface and re-attach the kernel driver, if it was previously connected.
    #[must_use]
    pub fn release(self) -> T2VModule<Opened> {
        ::std::mem::drop(self.state.interface);
        T2VModule {
            state: Opened {
                device: self.state.device,
            },
        }
    }
}

/// Wrapper for reading IR NEC frames from a USB endpoint.
pub struct IrNecReader {
    endpoint: EndpointRead<Interrupt>,
}

impl IrNecReader {
    /// Read the next IR NEC frame from the USB endpoint.
    ///
    /// # Errors
    ///
    /// This method fails, if reading from the endpoint fails. See [`Endpoint::read`](nusb::Endpoint::read) for more details.
    pub async fn next(&mut self) -> Result<Option<IrNecFrame>, io::Error> {
        let mut data = [0u8; 4];
        let len = self.endpoint.read(&mut data).await?;
        debug_assert_eq!(len, 4, "IR NEC data is always exactly 4 bytes");
        debug!("read data: {:02x?}", &data[..4]);

        Ok(Some(IrNecFrame {
            address: [data[0], data[1]],
            command: [data[2], data[3]],
        }))
    }
}

/// Frame of IR NEC data.
///
/// Conventional IR NEC frames consist of an address and a command, each sent once normally and inverted.
///
/// Extended frames can use both bytes of the address and command individually, at the expense of
/// no/limited error correction capabilities.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[repr(packed, C)]
pub struct IrNecFrame {
    address: [u8; 2],
    command: [u8; 2],
}

impl IrNecFrame {
    /// Check if the frame contains correct conventional frame data.
    #[must_use]
    pub fn is_correct_conventional(&self) -> bool {
        self.address[0] == !self.address[1] && self.command[0] == !self.command[1]
    }
    /// Address of the IR NEC frame.
    #[must_use]
    pub fn address(&self) -> [u8; 2] {
        self.address
    }
    /// Command of the IR NEC frame.
    #[must_use]
    pub fn command(&self) -> [u8; 2] {
        self.command
    }
}

#[cfg(test)]
mod tests {
    use crate::IrNecFrame;

    #[test]
    fn correct_frame() {
        assert!(
            IrNecFrame {
                address: [0x00, 0xff],
                command: [0x00, 0xff],
            }
            .is_correct_conventional()
        );
    }

    #[test]
    fn incorrect_frame() {
        assert!(
            !IrNecFrame {
                address: [0x01, 0xff],
                command: [0x00, 0xff],
            }
            .is_correct_conventional()
        );
    }
}
