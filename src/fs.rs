use esp_idf_svc::sys::{
    esp_vfs_fat_sdspi_mount, esp_vfs_fat_sdmmc_mount_config_t, sdmmc_card_t, sdmmc_host_t,
    sdspi_device_config_t, spi_bus_config_t, spi_host_device_t,
};
use std::ffi::{CString, CStr};
use std::ptr;
use log::info;

pub struct SdCard {
    card: *mut sdmmc_card_t,
    base_path: CString,
}

impl SdCard {
    pub fn new(
        mount_path: &str,
        spi_host: spi_host_device_t,
        miso: i32,
        mosi: i32,
        sclk: i32,
        cs: i32,
    ) -> Result<Self, esp_idf_svc::sys::EspError> {
        let mount_path_c = CString::new(mount_path).unwrap();
        
        // Use default configuration provided by ESP-IDF macro equivalents or manual construction
        // sdspi_host_init is often not exposed directly, but we can assume default SdspiHost config if we can find it.
        // Actually, for Rust, manually constructing is painful if symbols are missing.
        // Let's use `sdmmc_host_t::default()` if possible, but it's C struct.
        // We set only what we need.
        
        let mut host_config: sdmmc_host_t = Default::default();
        // SDMMC_HOST_FLAG_SPI is usually BIT(3) -> 8
        host_config.flags = 8 as _;
        host_config.slot = spi_host as _;
        host_config.max_freq_khz = 20000;
        host_config.io_voltage = 3.3;
        host_config.init = Some(esp_idf_svc::sys::sdspi_host_init);
        host_config.set_card_clk = Some(esp_idf_svc::sys::sdspi_host_set_card_clk);
        host_config.do_transaction = Some(esp_idf_svc::sys::sdspi_host_do_transaction);
        host_config.__bindgen_anon_1.deinit = Some(esp_idf_svc::sys::sdspi_host_deinit);
        host_config.io_int_enable = Some(esp_idf_svc::sys::sdspi_host_io_int_enable);
        host_config.io_int_wait = Some(esp_idf_svc::sys::sdspi_host_io_int_wait);
        host_config.get_real_freq = Some(esp_idf_svc::sys::sdspi_host_get_real_freq);
        
        // These might be missing if features are not enabled.
        // If compilation fails again on `sdspi_host_init`, we likely need to enable `sdspi` in sdkconfig?
        // But `esp-idf-sys` should have it if derived.
        // Let's assume the previous error was mostly due to me manually listing them as `esp_idf_svc::sys::...`.
        // Wait, the previous error was `struct spi_bus_config_t has no field named quadwp_io_num`.
        // It did NOT complain about `sdspi_host_init` in the output I saw?
        // Let's dry run.
        // Update: I will check the unions for bus_config.

        let mut slot_config = sdspi_device_config_t {
            host_id: spi_host,
            gpio_cs: cs,
            gpio_cd: -1,
            gpio_wp: -1,
            gpio_int: -1,
            ..Default::default()
        };

        let mount_config = esp_vfs_fat_sdmmc_mount_config_t {
            format_if_mount_failed: false,
            max_files: 5,
            allocation_unit_size: 16 * 1024,
            disk_status_check_enable: false,
        };

        // spi_bus_config_t union handling
        // We let Default handle the unions we don't touch, and we manually set valid ones.
        // But bindgen unions need safe access or manual construction.
        // Default::default() usually zeros it out.
        
        let mut bus_config: spi_bus_config_t = Default::default();
        // mosi (data0)
        // miso (data1)
        // sclk
        // quadwp (data2)
        // quadhd (data3)
        // data4..7
        
        bus_config.sclk_io_num = sclk;
        bus_config.__bindgen_anon_1.mosi_io_num = mosi;
        bus_config.__bindgen_anon_2.miso_io_num = miso;
        bus_config.__bindgen_anon_3.quadwp_io_num = -1;
        bus_config.__bindgen_anon_4.quadhd_io_num = -1;
        bus_config.max_transfer_sz = 4000;

        unsafe {
             info!("Initializing SPI bus...");
             let ret = esp_idf_svc::sys::spi_bus_initialize(
                spi_host,
                &bus_config,
                esp_idf_svc::sys::spi_common_dma_t_SPI_DMA_CH_AUTO,
            );
            if ret != 0 {
                return Err(esp_idf_svc::sys::EspError::from(ret).unwrap());
            }
        }

        let mut card: *mut sdmmc_card_t = ptr::null_mut();

        info!("Mounting SD card...");
        let ret = unsafe {
            esp_vfs_fat_sdspi_mount(
                mount_path_c.as_ptr(),
                &host_config,
                &slot_config,
                &mount_config,
                &mut card,
            )
        };

        if ret != 0 {
             info!("Mount failed: {}", ret);
             return Err(esp_idf_svc::sys::EspError::from(ret).unwrap());
        }

        info!("SD Card mounted at {}", mount_path);

        Ok(Self {
            card,
            base_path: mount_path_c,
        })
    }
}

impl Drop for SdCard {
    fn drop(&mut self) {
        unsafe {
            esp_idf_svc::sys::esp_vfs_fat_sdcard_unmount(
                self.base_path.as_ptr(),
                self.card
            );
        }
    }
}
