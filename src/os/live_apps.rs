use std::path::PathBuf;

use crate::keyboard::{CardputerKeyboard, Key, KeyEvent};
use crate::swapchain::DoubleBuffer;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveAppKind {
    // Empty for now
}

#[derive(Debug)]
pub enum LiveAppError {
    LoadFailed(String),
    RuntimeFailed(String),
}

pub enum LiveAppOutcome {
    Continue,
    Exit,
}

pub struct LiveAppRunner;

impl LiveAppRunner {
    pub fn load(
        _kind: LiveAppKind,
        _path: PathBuf,
        _speaker: esp_idf_hal::i2s::I2sDriver<'static, esp_idf_hal::i2s::I2sTx>,
    ) -> Result<Self, LiveAppError> {
        Err(LiveAppError::LoadFailed(
            "No live apps supported".to_string(),
        ))
    }

    pub fn tick(
        &mut self,
        _buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
        _keyboard: &mut CardputerKeyboard<'static>,
        _injected_key: Option<(KeyEvent, Key)>,
    ) -> Result<LiveAppOutcome, LiveAppError> {
        Ok(LiveAppOutcome::Exit)
    }

    pub fn teardown(self) -> Option<esp_idf_hal::i2s::I2sDriver<'static, esp_idf_hal::i2s::I2sTx>> {
        None
    }
}
