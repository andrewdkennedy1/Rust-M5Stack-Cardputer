#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use roxide_wasm::{
    clear, draw_text, key_from_code, key_to_char, Key, COLOR_BLACK, COLOR_CYAN, COLOR_WHITE,
};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

const MAX_LINES: usize = 6;
const MAX_INPUT: usize = 48;

struct AppState {
    input: String,
    lines: Vec<String>,
}

static mut STATE: Option<AppState> = None;

#[no_mangle]
pub extern "C" fn app_init() {
    unsafe {
        STATE = Some(AppState {
            input: String::new(),
            lines: Vec::new(),
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

    match key {
        Key::Enter => handle_enter(state),
        Key::Backspace => {
            state.input.pop();
        }
        _ => {
            if let Some(ch) = key_to_char(key) {
                if state.input.len() < MAX_INPUT {
                    state.input.push(ch);
                }
            }
        }
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
    draw_text(4, 4, "Rink Lite", COLOR_CYAN);

    let mut y = 18;
    for line in state.lines.iter().rev().take(MAX_LINES).rev() {
        draw_text(4, y, line, COLOR_WHITE);
        y += 10;
    }

    let mut prompt = String::from("> ");
    prompt.push_str(&state.input);
    draw_text(4, 118, &prompt, COLOR_WHITE);
}

#[no_mangle]
pub extern "C" fn app_framebuffer_ptr() -> i32 {
    roxide_wasm::framebuffer_ptr() as i32
}

#[no_mangle]
pub extern "C" fn app_framebuffer_len() -> i32 {
    roxide_wasm::framebuffer_len_bytes() as i32
}

fn handle_enter(state: &mut AppState) {
    if state.input.trim().is_empty() {
        return;
    }

    let input = core::mem::take(&mut state.input);
    let mut entered = String::from("> ");
    entered.push_str(&input);
    push_line(state, entered);

    let result = match eval_expression(&input) {
        Ok(value) => {
            let mut out = String::new();
            let _ = write!(&mut out, "{}", value);
            out
        }
        Err(err) => String::from(err),
    };

    push_line(state, result);
}

fn push_line(state: &mut AppState, line: String) {
    if state.lines.len() >= MAX_LINES {
        state.lines.remove(0);
    }
    state.lines.push(line);
}

fn eval_expression(input: &str) -> Result<f32, &'static str> {
    let mut parser = Parser::new(input.as_bytes());
    let value = parser.parse_expr()?;
    parser.skip_ws();
    if parser.pos < parser.bytes.len() {
        return Err("extra input");
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn parse_expr(&mut self) -> Result<f32, &'static str> {
        let mut value = self.parse_term()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'+') => {
                    self.pos += 1;
                    value += self.parse_term()?;
                }
                Some(b'-') => {
                    self.pos += 1;
                    value -= self.parse_term()?;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn parse_term(&mut self) -> Result<f32, &'static str> {
        let mut value = self.parse_factor()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'*') => {
                    self.pos += 1;
                    value *= self.parse_factor()?;
                }
                Some(b'/') => {
                    self.pos += 1;
                    let denom = self.parse_factor()?;
                    if denom == 0.0 {
                        return Err("division by zero");
                    }
                    value /= denom;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn parse_factor(&mut self) -> Result<f32, &'static str> {
        self.skip_ws();
        match self.peek() {
            Some(b'-') => {
                self.pos += 1;
                Ok(-self.parse_factor()?)
            }
            Some(b'(') => {
                self.pos += 1;
                let value = self.parse_expr()?;
                self.skip_ws();
                if self.peek() != Some(b')') {
                    return Err("missing )");
                }
                self.pos += 1;
                Ok(value)
            }
            _ => self.parse_number(),
        }
    }

    fn parse_number(&mut self) -> Result<f32, &'static str> {
        self.skip_ws();
        let start = self.pos;
        let mut saw_digit = false;
        while let Some(c) = self.peek() {
            if (c as char).is_ascii_digit() || c == b'.' {
                saw_digit = true;
                self.pos += 1;
            } else {
                break;
            }
        }
        if !saw_digit {
            return Err("expected number");
        }
        let token = core::str::from_utf8(&self.bytes[start..self.pos]).map_err(|_| "bad number")?;
        token.parse::<f32>().map_err(|_| "bad number")
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\t' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
}
