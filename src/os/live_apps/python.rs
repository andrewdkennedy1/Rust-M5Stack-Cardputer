use std::ffi::CString;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::time::Duration;

use crate::keyboard::{key_code, key_event_code, Key, KeyEvent};
use crate::swapchain::DoubleBuffer;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::{tick_interval, LiveAppError, LiveAppOutcome};

// 0 lets the runtime auto-size the heap based on free memory.
const DEFAULT_PY_HEAP_BYTES: usize = 0;

extern "C" {
    fn cardputer_mpy_start(path: *const c_char, heap_size: usize) -> i32;
    fn cardputer_mpy_start_mpy(path: *const c_char, heap_size: usize) -> i32;
    fn cardputer_mpy_tick(
        dt_ms: u32,
        key_code: i32,
        key_event: i32,
        framebuffer: *mut u16,
    ) -> i32;
    fn cardputer_mpy_stop();
    fn cardputer_mpy_last_error() -> *const c_char;
}

pub struct PythonApp;

impl PythonApp {
    pub fn load(path: PathBuf) -> Result<Self, LiveAppError> {
        let is_mpy = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("mpy"))
            .unwrap_or(false);
        let path_string = path.to_string_lossy().to_string();
        let c_path = CString::new(path_string)
            .map_err(|_| LiveAppError::LoadFailed("path contains null".to_string()))?;
        let result = unsafe {
            if is_mpy {
                cardputer_mpy_start_mpy(c_path.as_ptr(), DEFAULT_PY_HEAP_BYTES)
            } else {
                cardputer_mpy_start(c_path.as_ptr(), DEFAULT_PY_HEAP_BYTES)
            }
        };
        if result != 0 {
            return Err(LiveAppError::LoadFailed(last_error_message()));
        }
        Ok(Self)
    }

    pub fn tick(
        &mut self,
        buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
        input: Option<(KeyEvent, Key)>,
        dt: Duration,
    ) -> Result<LiveAppOutcome, LiveAppError> {
        let (key_code, key_event) = match input {
            Some((event, key)) => (key_code(key) as i32, key_event_code(event) as i32),
            None => (-1, 0),
        };
        let dt_ms = tick_interval(dt);

        let fbuf = buffers.swap_framebuffer();
        let result = unsafe { cardputer_mpy_tick(dt_ms, key_code, key_event, fbuf.framebuffer as *mut u16) };
        buffers.send_framebuffer();

        match result {
            0 => Ok(LiveAppOutcome::Continue),
            1 => Ok(LiveAppOutcome::Exit),
            _ => Err(LiveAppError::RuntimeFailed(last_error_message())),
        }
    }
}

impl Drop for PythonApp {
    fn drop(&mut self) {
        unsafe { cardputer_mpy_stop() };
    }
}

fn last_error_message() -> String {
    let c_str = unsafe { cardputer_mpy_last_error() };
    if c_str.is_null() {
        return "python error".to_string();
    }
    let message = unsafe { std::ffi::CStr::from_ptr(c_str) };
    let message = message.to_string_lossy().to_string();
    if message.is_empty() {
        "python error".to_string()
    } else {
        message
    }
}
