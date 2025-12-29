use esp_idf_svc::sys;
use log::info;
use std::ffi::{c_void, CString};

pub struct SdCard {
    card: *mut sys::sdmmc_card_t,
    base_path: CString,
    #[cfg(feature = "usb_msc")]
    sdspi_handle: sys::sdspi_dev_handle_t,
}

impl SdCard {
    #[cfg(feature = "usb_msc")]
    pub fn new(
        mount_path: &str,
        spi_host: sys::spi_host_device_t,
        miso: i32,
        mosi: i32,
        sclk: i32,
        cs: i32,
    ) -> Result<Self, sys::EspError> {
        let mount_path_c = CString::new(mount_path).unwrap();

        let mut bus_config: sys::spi_bus_config_t = Default::default();
        bus_config.sclk_io_num = sclk;
        bus_config.__bindgen_anon_1.mosi_io_num = mosi;
        bus_config.__bindgen_anon_2.miso_io_num = miso;
        bus_config.__bindgen_anon_3.quadwp_io_num = -1;
        bus_config.__bindgen_anon_4.quadhd_io_num = -1;
        bus_config.max_transfer_sz = 4000;

        unsafe {
            info!("Initializing SPI bus...");
            let ret = sys::spi_bus_initialize(
                spi_host,
                &bus_config,
                sys::spi_common_dma_t_SPI_DMA_CH_AUTO,
            );
            if ret != 0 {
                return Err(sys::EspError::from(ret).unwrap());
            }

            let ret = sys::sdspi_host_init();
            if ret != 0 {
                return Err(sys::EspError::from(ret).unwrap());
            }
        }

        let slot_config = sys::sdspi_device_config_t {
            host_id: spi_host,
            gpio_cs: cs,
            gpio_cd: -1,
            gpio_wp: -1,
            gpio_int: -1,
            ..Default::default()
        };

        let mut handle: sys::sdspi_dev_handle_t = 0;
        let ret = unsafe { sys::sdspi_host_init_device(&slot_config, &mut handle) };
        if ret != 0 {
            return Err(sys::EspError::from(ret).unwrap());
        }

        let mut host_config: sys::sdmmc_host_t = Default::default();
        host_config.flags = 8 as _;
        host_config.slot = handle;
        host_config.max_freq_khz = 20000;
        host_config.io_voltage = 3.3;
        host_config.init = Some(sys::sdspi_host_init);
        host_config.set_card_clk = Some(sys::sdspi_host_set_card_clk);
        host_config.do_transaction = Some(sys::sdspi_host_do_transaction);
        host_config.__bindgen_anon_1.deinit = Some(sys::sdspi_host_deinit);
        host_config.io_int_enable = Some(sys::sdspi_host_io_int_enable);
        host_config.io_int_wait = Some(sys::sdspi_host_io_int_wait);
        host_config.get_real_freq = Some(sys::sdspi_host_get_real_freq);

        let mut card = Box::new(unsafe { std::mem::zeroed::<sys::sdmmc_card_t>() });

        info!("Mounting SD card...");
        let ret = unsafe { sys::sdmmc_card_init(&host_config, card.as_mut()) };
        if ret != 0 {
            info!("SD card init failed: {}", ret);
            return Err(sys::EspError::from(ret).unwrap());
        }

        let mut storage_cfg: sys::tinyusb_msc_sdmmc_config_t =
            unsafe { std::mem::zeroed() };
        storage_cfg.card = card.as_mut() as *mut c_void;
        let ret = unsafe { sys::tinyusb_msc_storage_init_sdmmc(&storage_cfg) };
        if ret != 0 {
            info!("USB MSC storage init failed: {}", ret);
            return Err(sys::EspError::from(ret).unwrap());
        }

        let ret = unsafe { sys::tinyusb_msc_storage_mount(mount_path_c.as_ptr()) };
        if ret != 0 {
            info!("USB MSC mount failed: {}", ret);
            return Err(sys::EspError::from(ret).unwrap());
        }

        info!("SD Card mounted at {}", mount_path);

        Ok(Self {
            card: Box::into_raw(card),
            base_path: mount_path_c,
            sdspi_handle: handle,
        })
    }

    #[cfg(not(feature = "usb_msc"))]
    pub fn new(
        mount_path: &str,
        spi_host: sys::spi_host_device_t,
        miso: i32,
        mosi: i32,
        sclk: i32,
        cs: i32,
    ) -> Result<Self, sys::EspError> {
        let mount_path_c = CString::new(mount_path).unwrap();

        let mut host_config: sys::sdmmc_host_t = Default::default();
        host_config.flags = 8 as _;
        host_config.slot = spi_host as _;
        host_config.max_freq_khz = 20000;
        host_config.io_voltage = 3.3;
        host_config.init = Some(sys::sdspi_host_init);
        host_config.set_card_clk = Some(sys::sdspi_host_set_card_clk);
        host_config.do_transaction = Some(sys::sdspi_host_do_transaction);
        host_config.__bindgen_anon_1.deinit = Some(sys::sdspi_host_deinit);
        host_config.io_int_enable = Some(sys::sdspi_host_io_int_enable);
        host_config.io_int_wait = Some(sys::sdspi_host_io_int_wait);
        host_config.get_real_freq = Some(sys::sdspi_host_get_real_freq);

        let slot_config = sys::sdspi_device_config_t {
            host_id: spi_host,
            gpio_cs: cs,
            gpio_cd: -1,
            gpio_wp: -1,
            gpio_int: -1,
            ..Default::default()
        };

        let mount_config = sys::esp_vfs_fat_sdmmc_mount_config_t {
            format_if_mount_failed: false,
            max_files: 5,
            allocation_unit_size: 16 * 1024,
            disk_status_check_enable: false,
        };

        let mut bus_config: sys::spi_bus_config_t = Default::default();
        bus_config.sclk_io_num = sclk;
        bus_config.__bindgen_anon_1.mosi_io_num = mosi;
        bus_config.__bindgen_anon_2.miso_io_num = miso;
        bus_config.__bindgen_anon_3.quadwp_io_num = -1;
        bus_config.__bindgen_anon_4.quadhd_io_num = -1;
        bus_config.max_transfer_sz = 4000;

        unsafe {
            info!("Initializing SPI bus...");
            let ret = sys::spi_bus_initialize(
                spi_host,
                &bus_config,
                sys::spi_common_dma_t_SPI_DMA_CH_AUTO,
            );
            if ret != 0 {
                return Err(sys::EspError::from(ret).unwrap());
            }
        }

        let mut card: *mut sys::sdmmc_card_t = std::ptr::null_mut();

        info!("Mounting SD card...");
        let ret = unsafe {
            sys::esp_vfs_fat_sdspi_mount(
                mount_path_c.as_ptr(),
                &host_config,
                &slot_config,
                &mount_config,
                &mut card,
            )
        };

        if ret != 0 {
            info!("Mount failed: {}", ret);
            return Err(sys::EspError::from(ret).unwrap());
        }

        info!("SD Card mounted at {}", mount_path);

        Ok(Self {
            card,
            base_path: mount_path_c,
        })
    }

    pub fn card_ptr(&self) -> *mut sys::sdmmc_card_t {
        self.card
    }
}

#[cfg(feature = "usb_msc")]
impl Drop for SdCard {
    fn drop(&mut self) {
        unsafe {
            let _ = sys::tinyusb_msc_storage_unmount();
            sys::tinyusb_msc_storage_deinit();
            sys::sdspi_host_remove_device(self.sdspi_handle);
            sys::sdspi_host_deinit();
        }
    }
}

#[cfg(not(feature = "usb_msc"))]
impl Drop for SdCard {
    fn drop(&mut self) {
        unsafe {
            sys::esp_vfs_fat_sdcard_unmount(self.base_path.as_ptr(), self.card);
        }
    }
}
