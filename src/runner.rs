use std::ffi::CString;
use std::os::raw::c_char;
use std::path::Path;
use std::time::{Duration, Instant};

use embedded_gfx::framebuffer::DmaReadyFramebuffer;
use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use log::error;

use crate::display_driver::FramebufferTarget;
use crate::hal::CardputerPeripherals;
use crate::keyboard::{key_code, key_event_code, Key, KeyEvent};
use crate::os::chainload;
use crate::os::storage::mount_sd_card;
use crate::run_request;
use crate::runtime;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

const FRAMEBUFFER_PIXELS: usize = SCREEN_WIDTH * SCREEN_HEIGHT;
const TICK_SLEEP_MS: u64 = 16;
const PY_HEAP_REQUEST_BYTES: usize = 256 * 1024;

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

pub fn run() -> ! {
    runtime::init();
    let (cardputer, _modem) = runtime::take_cardputer();
    let CardputerPeripherals {
        mut display,
        mut keyboard,
        speaker: _,
    } = cardputer;

    let mut framebuffer = vec![0u16; FRAMEBUFFER_PIXELS];

    let sd = mount_sd_card();
    if sd.is_none() {
        error!("python runner: SD card not mounted");
        show_message(
            &mut display,
            &mut framebuffer,
            "Python Runner",
            &["SD card not mounted.", "Return to menu..."],
        );
        std::thread::sleep(Duration::from_millis(1200));
        chainload::reboot_to_factory();
    }
    let _sd = sd;

    let script_path = match run_request::read_run_request().ok().flatten() {
        Some(path) => path,
        None => {
            error!("python runner: no script selected");
            show_message(
                &mut display,
                &mut framebuffer,
                "Python Runner",
                &["No script selected.", "Return to menu..."],
            );
            std::thread::sleep(Duration::from_millis(1200));
            chainload::reboot_to_factory();
        }
    };
    let _ = run_request::clear_run_request();

    let is_mpy = script_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("mpy"))
        .unwrap_or(false);

    if let Err(err) = start_python(&script_path, is_mpy) {
        error!("python runner: start failed: {}", err);
        show_message(
            &mut display,
            &mut framebuffer,
            "Python Error",
            &[&err],
        );
        std::thread::sleep(Duration::from_millis(1500));
        chainload::reboot_to_factory();
    }

    let mut last_tick = Instant::now();
    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick);
        last_tick = now;

        let input = keyboard.read_events();
        if matches!(input, Some((KeyEvent::Pressed, Key::Fn))) {
            break;
        }

        let (key_code, key_event) = match input {
            Some((event, key)) => (key_code(key) as i32, key_event_code(event) as i32),
            None => (-1, 0),
        };
        let dt_ms = dt.as_millis().min(u128::from(u32::MAX)) as u32;

        let result =
            unsafe { cardputer_mpy_tick(dt_ms, key_code, key_event, framebuffer.as_mut_ptr()) };
        let _ = display.eat_framebuffer(&framebuffer);

        match result {
            0 => {}
            1 => break,
            _ => {
                let message = last_error_message();
                error!("python runner: runtime error: {}", message);
                show_message(
                    &mut display,
                    &mut framebuffer,
                    "Python Error",
                    &[&message],
                );
                std::thread::sleep(Duration::from_millis(1500));
                break;
            }
        }

        std::thread::sleep(Duration::from_millis(TICK_SLEEP_MS));
    }

    unsafe { cardputer_mpy_stop() };
    chainload::reboot_to_factory();
}

fn start_python(path: &Path, is_mpy: bool) -> Result<(), String> {
    let path_string = path.to_string_lossy().to_string();
    let c_path = CString::new(path_string).map_err(|_| "path contains null".to_string())?;
    let result = unsafe {
        if is_mpy {
            cardputer_mpy_start_mpy(c_path.as_ptr(), PY_HEAP_REQUEST_BYTES)
        } else {
            cardputer_mpy_start(c_path.as_ptr(), PY_HEAP_REQUEST_BYTES)
        }
    };
    if result != 0 {
        return Err(last_error_message());
    }
    Ok(())
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

fn show_message(
    display: &mut impl FramebufferTarget,
    framebuffer: &mut [u16],
    title: &str,
    lines: &[&str],
) {
    let mut fbuf = DmaReadyFramebuffer::<SCREEN_WIDTH, SCREEN_HEIGHT>::new(
        framebuffer.as_mut_ptr() as *mut std::ffi::c_void,
        true,
    );
    let _ = fbuf.clear(Rgb565::BLACK);

    let title_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    Text::new(title, Point::new(2, 10), title_style)
        .draw(&mut fbuf)
        .ok();

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    for (idx, line) in lines.iter().enumerate() {
        let y = 28 + idx as i32 * 12;
        Text::new(line, Point::new(2, y), text_style)
            .draw(&mut fbuf)
            .ok();
    }

    let _ = display.eat_framebuffer(framebuffer);
}
