use std::ffi::{c_void, CStr};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use esp_idf_svc::sys;

use super::ui::{render_status, FlashProgress};
use crate::swapchain::DoubleBuffer;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

const FLASH_CHUNK_SIZE: usize = 4096;

fn esp_err_name(err: i32) -> String {
    unsafe { CStr::from_ptr(sys::esp_err_to_name(err)).to_string_lossy().into_owned() }
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
    let total = file
        .metadata()
        .map(|m| m.len() as usize)
        .map_err(FlashError::Metadata)?;

    let update = unsafe { sys::esp_ota_get_next_update_partition(core::ptr::null()) };
    if update.is_null() {
        return Err(FlashError::NoOtaPartition);
    }

    let part_size = unsafe { (*update).size as usize };
    if total > part_size {
        return Err(FlashError::FileTooLarge { total, part_size });
    }

    let mut preview = [0u8; FLASH_CHUNK_SIZE];
    let mut preview_len = file.read(&mut preview).map_err(FlashError::Read)?;
    if preview_len == 0 {
        return Err(FlashError::EmptyFile);
    }

    if preview[0] != 0xE9 {
        return Err(FlashError::InvalidMagic(preview[0]));
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
            total: Some(total),
        }),
    );

    let mut handle: sys::esp_ota_handle_t = 0;
    let err = unsafe { sys::esp_ota_begin(update, total, &mut handle) };
    if err != 0 {
        return Err(FlashError::OtaBegin(err));
    }

    let mut buf = preview;
    let mut written = 0usize;
    let mut last_render = Instant::now();
    let mut last_pct = None::<usize>;

    loop {
        if preview_len == 0 {
            preview_len = file.read(&mut buf).map_err(FlashError::Read)?;
            if preview_len == 0 {
                break;
            }
        }

        let err = unsafe { sys::esp_ota_write(handle, buf.as_ptr() as *const c_void, preview_len) };
        if err != 0 {
            return Err(FlashError::OtaWrite(err));
        }

        written += preview_len;
        let pct = Some((written.saturating_mul(100) / total).min(100));
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
                    total: Some(total),
                }),
            );
        }

        preview_len = 0;
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
    Metadata(std::io::Error),
    NoOtaPartition,
    EmptyFile,
    InvalidMagic(u8),
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
            FlashError::Metadata(err) => vec![format!("Metadata failed: {}", err)],
            FlashError::NoOtaPartition => vec![
                "No OTA partition available.".to_string(),
                "Check partitions.csv.".to_string(),
            ],
            FlashError::EmptyFile => vec!["App image is empty.".to_string()],
            FlashError::InvalidMagic(magic) => vec![format!(
                "Invalid app image header. Expected 0xE9, found 0x{magic:02X}."
            )],
            FlashError::FileTooLarge { total, part_size } => vec![
                format!("File too large: {} bytes", total),
                format!("Partition size: {} bytes", part_size),
            ],
            FlashError::OtaBegin(err) => vec![format!(
                "OTA begin failed: {} ({})",
                err,
                esp_err_name(*err)
            )],
            FlashError::OtaWrite(err) => vec![format!(
                "OTA write failed after erase: {} ({})",
                err,
                esp_err_name(*err)
            )],
            FlashError::OtaEnd(err) => vec![format!(
                "OTA end failed: {} ({})",
                err,
                esp_err_name(*err)
            )],
            FlashError::OtaSetBoot(err) => vec![format!(
                "Set boot partition failed: {} ({})",
                err,
                esp_err_name(*err)
            )],
        }
    }
}
