#[cfg(feature = "usb_msc")]
mod imp {
    use esp_idf_svc::sys::{self, EspError};
    use log::{error, info};

    use crate::fs::SdCard;

    pub struct UsbMsc {
        host_active: bool,
    }

    impl UsbMsc {
        pub fn init(_sd_card: &SdCard) -> Result<Self, EspError> {
            let tusb_cfg: sys::tinyusb_config_t = unsafe { std::mem::zeroed() };
            let err = unsafe { sys::tinyusb_driver_install(&tusb_cfg) };
            if err != 0 {
                error!("USB MSC init failed (driver): {}", err);
                return Err(EspError::from(err).unwrap());
            }

            info!("USB MSC initialized");
            let host_active = unsafe { sys::tinyusb_msc_storage_in_use_by_usb_host() };
            Ok(Self {
                host_active: host_active as u8 != 0,
            })
        }

        pub fn poll(&mut self) -> Option<bool> {
            let active = unsafe { sys::tinyusb_msc_storage_in_use_by_usb_host() };
            let active = active as u8 != 0;
            if active != self.host_active {
                self.host_active = active;
                return Some(active);
            }
            None
        }

        pub fn host_active(&self) -> bool {
            self.host_active
        }
    }
}

#[cfg(not(feature = "usb_msc"))]
mod imp {
    use esp_idf_svc::sys::{self, EspError};

    use crate::fs::SdCard;

    pub struct UsbMsc {
        host_active: bool,
    }

    impl UsbMsc {
        pub fn init(_sd_card: &SdCard) -> Result<Self, EspError> {
            Err(EspError::from(sys::ESP_ERR_NOT_SUPPORTED).unwrap())
        }

        pub fn poll(&mut self) -> Option<bool> {
            None
        }

        pub fn host_active(&self) -> bool {
            self.host_active
        }
    }
}

pub use imp::UsbMsc;
