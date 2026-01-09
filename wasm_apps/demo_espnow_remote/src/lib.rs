#![no_std]

extern crate alloc;

use alloc::string::String;

use roxide_wasm::{
    clear, draw_text, is_back, key_from_code, key_to_char, Key, COLOR_BLACK, COLOR_CYAN,
    COLOR_WHITE,
};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

struct AppState {
    peer_input: String,
    peer: [u8; 6],
    peer_ready: bool,
    peer_epoch: u32,
    outbox_len: usize,
    last_sent: Option<char>,
    status: String,
}

static mut STATE: Option<AppState> = None;
static mut OUTBOX: [u8; 16] = [0; 16];

#[no_mangle]
pub extern "C" fn app_init() {
    unsafe {
        STATE = Some(AppState {
            peer_input: String::new(),
            peer: [0; 6],
            peer_ready: false,
            peer_epoch: 0,
            outbox_len: 0,
            last_sent: None,
            status: String::from("Enter peer MAC"),
        });
    }
}

#[no_mangle]
pub extern "C" fn app_update(_dt_ms: i32, key_code: i32, key_event: i32) {
    let state = unsafe {
        if STATE.is_none() {
            app_init();
        }
        STATE.as_mut().unwrap()
    };

    if key_event != 1 {
        return;
    }

    let Some(key) = key_from_code(key_code) else {
        return;
    };

    if state.peer_ready {
        handle_send_mode(state, key);
    } else {
        handle_config_mode(state, key);
    }
}

#[no_mangle]
pub extern "C" fn app_render() {
    let state = unsafe {
        if STATE.is_none() {
            app_init();
        }
        STATE.as_ref().unwrap()
    };

    clear(COLOR_BLACK);
    draw_text(4, 4, "ESP-NOW Remote", COLOR_CYAN);
    draw_text(4, 18, &state.status, COLOR_WHITE);

    if state.peer_ready {
        let mac = format_mac(&state.peer);
        draw_text(4, 32, "Peer:", COLOR_WHITE);
        draw_text(44, 32, &mac, COLOR_WHITE);
        draw_text(4, 44, "Type to send", COLOR_WHITE);
        draw_text(4, 54, "Backspace: reset", COLOR_WHITE);
        if let Some(sent) = state.last_sent {
            let mut line = String::from("Last: ");
            line.push(sent);
            draw_text(4, 66, &line, COLOR_WHITE);
        }
    } else {
        draw_text(4, 32, "Format: AABBCCDDEEFF", COLOR_WHITE);
        draw_text(4, 44, &state.peer_input, COLOR_WHITE);
        draw_text(4, 54, "Enter: confirm", COLOR_WHITE);
    }
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
pub extern "C" fn app_network_out_ptr() -> i32 {
    unsafe { OUTBOX.as_ptr() as i32 }
}

#[no_mangle]
pub extern "C" fn app_network_out_len() -> i32 {
    unsafe {
        STATE
            .as_ref()
            .map(|s| s.outbox_len as i32)
            .unwrap_or(0)
    }
}

#[no_mangle]
pub extern "C" fn app_network_out_clear() {
    unsafe {
        if let Some(state) = STATE.as_mut() {
            state.outbox_len = 0;
        }
    }
}

#[no_mangle]
pub extern "C" fn app_network_peer_ptr() -> i32 {
    unsafe {
        STATE
            .as_ref()
            .map(|s| s.peer.as_ptr() as i32)
            .unwrap_or(0)
    }
}

#[no_mangle]
pub extern "C" fn app_network_peer_epoch() -> i32 {
    unsafe {
        STATE
            .as_ref()
            .map(|s| if s.peer_ready { s.peer_epoch as i32 } else { 0 })
            .unwrap_or(0)
    }
}

fn handle_config_mode(state: &mut AppState, key: Key) {
    if key == Key::Enter {
        if let Some(peer) = parse_mac(&state.peer_input) {
            state.peer = peer;
            state.peer_ready = true;
            state.peer_epoch = state.peer_epoch.wrapping_add(1).max(1);
            state.status = String::from("Peer set");
            state.peer_input.clear();
        } else {
            state.status = String::from("Invalid MAC");
        }
        return;
    }

    if is_back(key) {
        state.peer_input.pop();
        return;
    }

    if let Some(ch) = key_to_char(key) {
        if ch.is_ascii_hexdigit() || ch == ':' {
            if state.peer_input.len() < 17 {
                state.peer_input.push(ch.to_ascii_uppercase());
            }
        }
    }
}

fn handle_send_mode(state: &mut AppState, key: Key) {
    if is_back(key) {
        state.peer_ready = false;
        state.status = String::from("Enter peer MAC");
        state.last_sent = None;
        return;
    }

    if let Some(ch) = key_to_char(key) {
        unsafe {
            OUTBOX[0] = ch as u8;
        }
        state.outbox_len = 1;
        state.last_sent = Some(ch);
    }
}

fn parse_mac(input: &str) -> Option<[u8; 6]> {
    let mut hex = [0u8; 12];
    let mut count = 0;

    for ch in input.chars() {
        if ch == ':' {
            continue;
        }
        if !ch.is_ascii_hexdigit() {
            return None;
        }
        if count >= hex.len() {
            return None;
        }
        hex[count] = ch.to_ascii_uppercase() as u8;
        count += 1;
    }

    if count != 12 {
        return None;
    }

    let mut out = [0u8; 6];
    for i in 0..6 {
        let hi = hex_value(hex[i * 2])?;
        let lo = hex_value(hex[i * 2 + 1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn hex_value(ch: u8) -> Option<u8> {
    match ch {
        b'0'..=b'9' => Some(ch - b'0'),
        b'A'..=b'F' => Some(ch - b'A' + 10),
        _ => None,
    }
}

fn format_mac(peer: &[u8; 6]) -> String {
    let mut out = String::new();
    for (idx, byte) in peer.iter().enumerate() {
        let hi = hex_digit(byte >> 4);
        let lo = hex_digit(byte & 0x0f);
        out.push(hi);
        out.push(lo);
        if idx + 1 != peer.len() {
            out.push(':');
        }
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + (nibble - 10)) as char,
    }
}
