use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::keyboard::{CardputerKeyboard, Key, KeyEvent};
use crate::swapchain::DoubleBuffer;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

mod python;
mod wasm;

use python::PythonApp;
use wasm::WasmApp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveAppKind {
    Wasm,
    Python,
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

pub struct LiveAppRunner {
    app: LiveAppState,
    last_tick: Instant,
}

enum LiveAppState {
    Wasm(WasmApp),
    Python(PythonApp),
}

impl LiveAppState {
    fn teardown(self) -> Option<esp_idf_hal::i2s::I2sDriver<'static, esp_idf_hal::i2s::I2sTx>> {
        match self {
            LiveAppState::Wasm(_) => None,
            LiveAppState::Python(app) => app.teardown(),
        }
    }
}

impl LiveAppRunner {
    pub fn load(
        kind: LiveAppKind,
        path: PathBuf,
        speaker: esp_idf_hal::i2s::I2sDriver<'static, esp_idf_hal::i2s::I2sTx>,
    ) -> Result<Self, LiveAppError> {
        let app = match kind {
            LiveAppKind::Wasm => LiveAppState::Wasm(WasmApp::load(path)?),
            LiveAppKind::Python => LiveAppState::Python(PythonApp::load(path, speaker)?),
        };

        Ok(Self {
            app,
            last_tick: Instant::now(),
        })
    }

    pub fn tick(
        &mut self,
        buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
        keyboard: &mut CardputerKeyboard<'static>,
        injected_key: Option<(KeyEvent, Key)>,
    ) -> Result<LiveAppOutcome, LiveAppError> {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick);
        self.last_tick = now;

        let input = injected_key.or_else(|| keyboard.read_events());

        if let Some((KeyEvent::Pressed, Key::Fn)) = input {
            return Ok(LiveAppOutcome::Exit);
        }

        match &mut self.app {
            LiveAppState::Wasm(app) => app.tick(buffers, input, dt),
            LiveAppState::Python(app) => app.tick(buffers, input, dt),
        }
    }

    pub fn teardown(
        self,
    ) -> Option<esp_idf_hal::i2s::I2sDriver<'static, esp_idf_hal::i2s::I2sTx>> {
        self.app.teardown()
    }
}

pub(super) fn tick_interval(dt: Duration) -> u32 {
    dt.as_millis().min(u128::from(u32::MAX)) as u32
}
