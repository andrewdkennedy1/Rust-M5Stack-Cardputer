use std::ffi::c_void;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use esp_idf_svc::sys;

use super::ui::{render_status, FlashProgress};
use crate::swapchain::DoubleBuffer;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

const FLASH_CHUNK_SIZE: usize = 4096;

pub fn set_factory_boot_partition() -> bool {
    unsafe {
        let factory = sys::esp_partition_find_first(
            sys::esp_partition_type_t_ESP_PARTITION_TYPE_APP,
            sys::esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_APP_FACTORY,
            core::ptr::null(),
        );
        if !factory.is_null() {
            sys::esp_ota_set_boot_partition(factory);
            true
        } else {
            false
        }
    }
}

pub fn reboot_to_factory() -> ! {
    let _ = set_factory_boot_partition();
    unsafe { sys::esp_restart() }
}

pub fn ota_partition_available() -> bool {
    let update = unsafe { sys::esp_ota_get_next_update_partition(core::ptr::null()) };
    !update.is_null()
}

pub fn flash_and_reboot(
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    path: &Path,
) -> Result<(), FlashError> {
    let mut file = File::open(path).map_err(FlashError::Open)?;
    let total = file.metadata().ok().map(|m| m.len() as usize);

    let update = unsafe { sys::esp_ota_get_next_update_partition(core::ptr::null()) };
    if update.is_null() {
        return Err(FlashError::NoOtaPartition);
    }

    let part_size = unsafe { (*update).size as usize };
    if let Some(total) = total {
        if total > part_size {
            return Err(FlashError::FileTooLarge { total, part_size });
        }
    }

    render_status(
        buffers,
        "Flashing",
        &[path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("app.bin")],
        Some(FlashProgress {
            written: 0,
            total,
        }),
    );

    let mut handle: sys::esp_ota_handle_t = 0;
    let err = unsafe { sys::esp_ota_begin(update, sys::OTA_SIZE_UNKNOWN as usize, &mut handle) };
    if err != 0 {
        return Err(FlashError::OtaBegin(err));
    }

    let mut buf = [0u8; FLASH_CHUNK_SIZE];
    let mut written = 0usize;
    let mut last_render = Instant::now();
    let mut last_pct = None::<usize>;

    loop {
        let n = match file.read(&mut buf) {
            Ok(n) => n,
            Err(err) => {
                return Err(FlashError::Read(err));
            }
        };
        if n == 0 {
            break;
        }

        let err = unsafe { sys::esp_ota_write(handle, buf.as_ptr() as *const c_void, n) };
        if err != 0 {
            return Err(FlashError::OtaWrite(err));
        }

        written += n;
        let pct = total.map(|t| (written.saturating_mul(100) / t).min(100));
        let should_render = match (pct, last_pct) {
            (Some(pct), Some(last)) => pct != last,
            (Some(_), None) => true,
            (None, _) => last_render.elapsed() > Duration::from_millis(150),
        };

        if should_render || last_render.elapsed() > Duration::from_millis(150) {
            last_render = Instant::now();
            last_pct = pct;
            render_status(
                buffers,
                "Flashing",
                &[path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("app.bin")],
                Some(FlashProgress {
                    written,
                    total,
                }),
            );
        }
    }

    let err = unsafe { sys::esp_ota_end(handle) };
    if err != 0 {
        return Err(FlashError::OtaEnd(err));
    }

    let err = unsafe { sys::esp_ota_set_boot_partition(update) };
    if err != 0 {
        return Err(FlashError::OtaSetBoot(err));
    }

    render_status(buffers, "Rebooting", &["Switching app..."], None);
    std::thread::sleep(Duration::from_millis(500));
    unsafe { sys::esp_restart() }
}

#[derive(Debug)]
pub enum FlashError {
    Open(std::io::Error),
    Read(std::io::Error),
    NoOtaPartition,
    FileTooLarge { total: usize, part_size: usize },
    OtaBegin(i32),
    OtaWrite(i32),
    OtaEnd(i32),
    OtaSetBoot(i32),
}

impl FlashError {
    pub fn to_lines(&self) -> Vec<String> {
        match self {
            FlashError::Open(err) => vec![format!("Open failed: {}", err)],
            FlashError::Read(err) => vec![format!("Read failed: {}", err)],
            FlashError::NoOtaPartition => vec![
                "No OTA partition available.".to_string(),
                "Check partitions.csv.".to_string(),
            ],
            FlashError::FileTooLarge { total, part_size } => vec![
                format!("File too large: {} bytes", total),
                format!("Partition size: {} bytes", part_size),
            ],
            FlashError::OtaBegin(err) => vec![format!("OTA begin failed: {}", err)],
            FlashError::OtaWrite(err) => vec![format!("OTA write failed: {}", err)],
            FlashError::OtaEnd(err) => vec![format!("OTA end failed: {}", err)],
            FlashError::OtaSetBoot(err) => vec![format!("Set boot failed: {}", err)],
        }
    }
}
