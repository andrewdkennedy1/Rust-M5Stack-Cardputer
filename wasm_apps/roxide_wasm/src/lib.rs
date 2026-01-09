#![no_std]

use font8x8::UnicodeFonts;

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 135;
const FB_SIZE: usize = WIDTH * HEIGHT;

static mut FRAMEBUFFER: [u16; FB_SIZE] = [0; FB_SIZE];

pub const COLOR_BLACK: u16 = 0x0000;
pub const COLOR_WHITE: u16 = 0xFFFF;
pub const COLOR_RED: u16 = 0xF800;
pub const COLOR_GREEN: u16 = 0x07E0;
pub const COLOR_BLUE: u16 = 0x001F;
pub const COLOR_YELLOW: u16 = 0xFFE0;
pub const COLOR_CYAN: u16 = 0x07FF;
pub const COLOR_MAGENTA: u16 = 0xF81F;

pub fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    let r = (r as u16 >> 3) & 0x1f;
    let g = (g as u16 >> 2) & 0x3f;
    let b = (b as u16 >> 3) & 0x1f;
    (r << 11) | (g << 5) | b
}

pub fn framebuffer_ptr() -> *const u16 {
    unsafe { FRAMEBUFFER.as_ptr() }
}

pub fn framebuffer_len_bytes() -> usize {
    FB_SIZE * 2
}

pub fn clear(color: u16) {
    unsafe {
        for px in FRAMEBUFFER.iter_mut() {
            *px = color;
        }
    }
}

pub fn set_pixel(x: i32, y: i32, color: u16) {
    if x < 0 || y < 0 {
        return;
    }
    let x = x as usize;
    let y = y as usize;
    if x >= WIDTH || y >= HEIGHT {
        return;
    }
    unsafe {
        FRAMEBUFFER[y * WIDTH + x] = color;
    }
}

pub fn draw_line(mut x0: i32, mut y0: i32, mut x1: i32, mut y1: i32, color: u16) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        set_pixel(x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

pub fn draw_rect(x: i32, y: i32, w: i32, h: i32, color: u16) {
    for yy in y..(y + h) {
        for xx in x..(x + w) {
            set_pixel(xx, yy, color);
        }
    }
}

pub fn draw_char(x: i32, y: i32, c: char, color: u16) {
    if let Some(glyph) = font8x8::BASIC_FONTS.get(c) {
        for (row, bits) in glyph.iter().enumerate() {
            let y = y + row as i32;
            for col in 0..8 {
                if (bits >> col) & 1 != 0 {
                    set_pixel(x + col as i32, y, color);
                }
            }
        }
    }
}

pub fn draw_text(mut x: i32, mut y: i32, text: &str, color: u16) {
    let start_x = x;
    for c in text.chars() {
        if c == '\n' {
            y += 9;
            x = start_x;
            continue;
        }
        draw_char(x, y, c, color);
        x += 8;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Space,
    Period,
    M,
    B,
    C,
    Z,
    Opt,
    Enter,
    Semicolon,
    K,
    H,
    F,
    S,
    Shift,
    BackSlash,
    LeftSquareBracket,
    O,
    U,
    T,
    E,
    Q,
    Backspace,
    Underscore,
    _9,
    _7,
    _5,
    _3,
    _1,
    Slash,
    Comma,
    N,
    V,
    X,
    Alt,
    Ctrl,
    Quote,
    L,
    J,
    G,
    D,
    A,
    Fn,
    RightSquareBracket,
    P,
    I,
    Y,
    R,
    W,
    Tab,
    Equal,
    _0,
    _8,
    _6,
    _4,
    _2,
    Tilde,
}

const KEY_MAP: [Key; 56] = [
    Key::Opt,
    Key::Z,
    Key::C,
    Key::B,
    Key::M,
    Key::Period,
    Key::Space,
    Key::Shift,
    Key::S,
    Key::F,
    Key::H,
    Key::K,
    Key::Semicolon,
    Key::Enter,
    Key::Q,
    Key::E,
    Key::T,
    Key::U,
    Key::O,
    Key::LeftSquareBracket,
    Key::BackSlash,
    Key::_1,
    Key::_3,
    Key::_5,
    Key::_7,
    Key::_9,
    Key::Underscore,
    Key::Backspace,
    Key::Ctrl,
    Key::Alt,
    Key::X,
    Key::V,
    Key::N,
    Key::Comma,
    Key::Slash,
    Key::Fn,
    Key::A,
    Key::D,
    Key::G,
    Key::J,
    Key::L,
    Key::Quote,
    Key::Tab,
    Key::W,
    Key::R,
    Key::Y,
    Key::I,
    Key::P,
    Key::RightSquareBracket,
    Key::Tilde,
    Key::_2,
    Key::_4,
    Key::_6,
    Key::_8,
    Key::_0,
    Key::Equal,
];

pub fn key_from_code(code: i32) -> Option<Key> {
    if code < 0 {
        return None;
    }
    KEY_MAP.get(code as usize).copied()
}

pub fn key_to_char(key: Key) -> Option<char> {
    match key {
        Key::Space => Some(' '),
        Key::Period => Some('.'),
        Key::Comma => Some(','),
        Key::Slash => Some('/'),
        Key::Semicolon => Some(';'),
        Key::Quote => Some('\''),
        Key::BackSlash => Some('\\'),
        Key::LeftSquareBracket => Some('['),
        Key::RightSquareBracket => Some(']'),
        Key::Tilde => Some('`'),
        Key::Equal => Some('='),
        Key::Underscore => Some('_'),
        Key::_0 => Some('0'),
        Key::_1 => Some('1'),
        Key::_2 => Some('2'),
        Key::_3 => Some('3'),
        Key::_4 => Some('4'),
        Key::_5 => Some('5'),
        Key::_6 => Some('6'),
        Key::_7 => Some('7'),
        Key::_8 => Some('8'),
        Key::_9 => Some('9'),
        Key::A => Some('a'),
        Key::B => Some('b'),
        Key::C => Some('c'),
        Key::D => Some('d'),
        Key::E => Some('e'),
        Key::F => Some('f'),
        Key::G => Some('g'),
        Key::H => Some('h'),
        Key::I => Some('i'),
        Key::J => Some('j'),
        Key::K => Some('k'),
        Key::L => Some('l'),
        Key::M => Some('m'),
        Key::N => Some('n'),
        Key::O => Some('o'),
        Key::P => Some('p'),
        Key::Q => Some('q'),
        Key::R => Some('r'),
        Key::S => Some('s'),
        Key::T => Some('t'),
        Key::U => Some('u'),
        Key::V => Some('v'),
        Key::W => Some('w'),
        Key::X => Some('x'),
        Key::Y => Some('y'),
        Key::Z => Some('z'),
        _ => None,
    }
}

pub fn is_up(key: Key) -> bool {
    matches!(key, Key::Semicolon | Key::W)
}

pub fn is_down(key: Key) -> bool {
    matches!(key, Key::Period | Key::S)
}

pub fn is_select(key: Key) -> bool {
    matches!(key, Key::Enter)
}

pub fn is_back(key: Key) -> bool {
    matches!(key, Key::Backspace | Key::Slash)
}
