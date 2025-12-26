use std::path::PathBuf;

/// Represents an application artifact that can be launched by the OS.
#[derive(Debug, Clone)]
pub struct AppLaunch {
    pub path: PathBuf,
}

impl AppLaunch {
    pub fn from_path(path: PathBuf) -> Self {
        Self { path }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AppContext {
    pub sd_ready: bool,
    pub ota_ready: bool,
}

impl AppContext {
    pub fn new(sd_ready: bool, ota_ready: bool) -> Self {
        Self { sd_ready, ota_ready }
    }

    pub fn validate_launch(&self, _launch: &AppLaunch) -> Result<(), AppValidationError> {
        if !self.sd_ready {
            return Err(AppValidationError::MissingSd);
        }
        if !self.ota_ready {
            return Err(AppValidationError::MissingOta);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AppValidationError {
    MissingSd,
    MissingOta,
}

impl AppValidationError {
    pub fn to_lines(&self) -> Vec<String> {
        match self {
            AppValidationError::MissingSd => {
                vec!["SD card not mounted.".to_string(), "Insert card and reboot.".to_string()]
            }
            AppValidationError::MissingOta => vec![
                "No OTA partitions found.".to_string(),
                "Update partitions.csv and rebuild.".to_string(),
            ],
        }
    }
}
