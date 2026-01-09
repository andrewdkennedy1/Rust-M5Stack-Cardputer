#![no_std]

use core::f32::consts::PI;

use libm::sinf;
use roxide_wasm::{
    clear, draw_text, key_from_code, key_to_char, COLOR_BLACK, COLOR_CYAN, COLOR_WHITE,
};

const SAMPLE_RATE: usize = 48_000;
const BEEP_SECONDS: usize = 1;
const AUDIO_LEN: usize = SAMPLE_RATE * BEEP_SECONDS;

static mut AUDIO_BUFFER: [u8; AUDIO_LEN] = [0; AUDIO_LEN];
static mut AUDIO_READY: bool = false;
static mut AUDIO_LEN_ACTIVE: usize = 0;

#[no_mangle]
pub extern "C" fn app_init() {
    unsafe {
        AUDIO_READY = false;
        AUDIO_LEN_ACTIVE = 0;
    }
}

#[no_mangle]
pub extern "C" fn app_update(_dt_ms: i32, key_code: i32, key_event: i32) {
    if key_event != 1 {
        return;
    }

    let Some(key) = key_from_code(key_code) else {
        return;
    };
    let trigger = matches!(key_to_char(key), Some('b') | Some(' '));
    if !trigger {
        return;
    }

    unsafe {
        if !AUDIO_READY {
            generate_beep();
            AUDIO_READY = true;
        }
        if AUDIO_LEN_ACTIVE == 0 {
            AUDIO_LEN_ACTIVE = AUDIO_BUFFER.len();
        }
    }
}

#[no_mangle]
pub extern "C" fn app_render() {
    clear(COLOR_BLACK);
    draw_text(4, 4, "Sound Demo", COLOR_CYAN);
    draw_text(4, 20, "Press B or Space", COLOR_WHITE);
    draw_text(4, 30, "to play a beep", COLOR_WHITE);
}

#[no_mangle]
pub extern "C" fn app_framebuffer_ptr() -> i32 {
    roxide_wasm::framebuffer_ptr() as i32
}

#[no_mangle]
pub extern "C" fn app_framebuffer_len() -> i32 {
    roxide_wasm::framebuffer_len_bytes() as i32
}

#[no_mangle]
pub extern "C" fn app_audio_ptr() -> i32 {
    unsafe { AUDIO_BUFFER.as_ptr() as i32 }
}

#[no_mangle]
pub extern "C" fn app_audio_len() -> i32 {
    unsafe { AUDIO_LEN_ACTIVE as i32 }
}

#[no_mangle]
pub extern "C" fn app_audio_clear() {
    unsafe {
        AUDIO_LEN_ACTIVE = 0;
    }
}

unsafe fn generate_beep() {
    let freq = 880.0;
    let amplitude = 127.0;
    let sample_period = 1.0 / SAMPLE_RATE as f32;

    for i in 0..AUDIO_BUFFER.len() {
        let t = i as f32 * sample_period;
        let sample = amplitude * sinf(2.0 * PI * freq * t);
        AUDIO_BUFFER[i] = sample as i8 as u8;
    }
}
