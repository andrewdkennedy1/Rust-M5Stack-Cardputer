use std::fs::read_dir;

use esp_idf_svc::sys;

use crate::fs::SdCard;

pub const SD_ROOT: &str = "/sdcard";
pub const SD_APPS_PATH: &str = "/sdcard/apps";
pub const SD_MODELS_PATH: &str = "/sdcard/3d";

pub struct SdFileEntry {
    pub name: String,
    pub path: String,
}

pub fn mount_sd_card() -> Option<SdCard> {
    SdCard::new(
        SD_ROOT,
        sys::spi_host_device_t_SPI3_HOST,
        39,
        14,
        40,
        12,
    )
    .ok()
}

pub fn list_files_with_extension(dir: &str, extension: &str) -> Vec<SdFileEntry> {
    let mut entries = Vec::new();
    let Ok(dir_iter) = read_dir(dir) else {
        return entries;
    };

    let ext_str = extension.trim_start_matches('.');
    for entry in dir_iter.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()).map_or(false, |ext| {
            ext.eq_ignore_ascii_case(ext_str)
        }) {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .to_string();
            entries.push(SdFileEntry {
                name,
                path: path.to_string_lossy().to_string(),
            });
        }
    }

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries
}
