#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::f32::consts::PI;

use libm::{cosf, sinf};
use roxide_wasm::{
    clear, draw_line, draw_rect, draw_text, is_back, is_down, is_select, is_up, key_from_code,
    Key, COLOR_BLACK, COLOR_CYAN, COLOR_GREEN, COLOR_WHITE, HEIGHT, WIDTH,
};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

struct Model {
    name: &'static str,
    bytes: &'static [u8],
}

const MODELS: [Model; 4] = [
    Model {
        name: "Suzanne",
        bytes: include_bytes!("../assets/Suzanne.stl"),
    },
    Model {
        name: "Teapot",
        bytes: include_bytes!("../assets/Teapot_low.stl"),
    },
    Model {
        name: "Blahaj",
        bytes: include_bytes!("../assets/blahaj.stl"),
    },
    Model {
        name: "LPcatBP",
        bytes: include_bytes!("../assets/LPcatBP.stl"),
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Picker,
    Viewer,
}

struct Mesh {
    vertices: Vec<[f32; 3]>,
    faces: Vec<[usize; 3]>,
    center: [f32; 3],
    scale: f32,
}

struct AppState {
    mode: Mode,
    selected: usize,
    mesh: Option<Mesh>,
    angle: f32,
    tilt: f32,
    should_exit: bool,
}

static mut STATE: Option<AppState> = None;

#[no_mangle]
pub extern "C" fn app_init() {
    unsafe {
        STATE = Some(AppState {
            mode: Mode::Picker,
            selected: 0,
            mesh: None,
            angle: 0.0,
            tilt: 0.4,
            should_exit: false,
        });
    }
}

#[no_mangle]
pub extern "C" fn app_update(dt_ms: i32, key_code: i32, key_event: i32) {
    let state = unsafe {
        if STATE.is_none() {
            app_init();
        }
        STATE.as_mut().unwrap()
    };

    state.angle += dt_ms as f32 * 0.0012;
    if state.angle > PI * 2.0 {
        state.angle -= PI * 2.0;
    }

    if key_event != 1 {
        return;
    }

    let Some(key) = key_from_code(key_code) else {
        return;
    };

    match state.mode {
        Mode::Picker => handle_picker_input(state, key),
        Mode::Viewer => handle_viewer_input(state, key),
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
    render(state);
}

#[no_mangle]
pub extern "C" fn app_should_exit() -> i32 {
    let state = unsafe { STATE.as_ref() };
    state.map(|s| s.should_exit as i32).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn app_framebuffer_ptr() -> i32 {
    roxide_wasm::framebuffer_ptr() as i32
}

#[no_mangle]
pub extern "C" fn app_framebuffer_len() -> i32 {
    roxide_wasm::framebuffer_len_bytes() as i32
}

fn handle_picker_input(state: &mut AppState, key: Key) {
    if is_up(key) {
        if state.selected == 0 {
            state.selected = MODELS.len() - 1;
        } else {
            state.selected -= 1;
        }
    } else if is_down(key) {
        state.selected = (state.selected + 1) % MODELS.len();
    } else if is_select(key) {
        let model = &MODELS[state.selected];
        state.mesh = parse_stl(model.bytes);
        state.mode = Mode::Viewer;
    } else if is_back(key) {
        state.should_exit = true;
    }
}

fn handle_viewer_input(state: &mut AppState, key: Key) {
    if is_back(key) {
        state.mode = Mode::Picker;
        return;
    }

    match key {
        Key::A => state.angle -= 0.2,
        Key::D => state.angle += 0.2,
        Key::W => state.tilt += 0.1,
        Key::S => state.tilt -= 0.1,
        _ => {}
    }
}

fn render(state: &AppState) {
    clear(COLOR_BLACK);
    draw_text(4, 4, "3D Demo", COLOR_CYAN);

    match state.mode {
        Mode::Picker => render_picker(state),
        Mode::Viewer => render_viewer(state),
    }
}

fn render_picker(state: &AppState) {
    draw_text(4, 18, "Select Model", COLOR_WHITE);
    for (idx, model) in MODELS.iter().enumerate() {
        let y = 32 + (idx as i32) * 12;
        if idx == state.selected {
            draw_rect(2, y - 2, 120, 10, COLOR_GREEN);
            draw_text(6, y, model.name, COLOR_BLACK);
        } else {
            draw_text(6, y, model.name, COLOR_WHITE);
        }
    }
    draw_text(4, 118, "Up/Down: ;/.  Enter: view", COLOR_WHITE);
    draw_text(4, 128, "Back: Backspace", COLOR_WHITE);
}

fn render_viewer(state: &AppState) {
    if let Some(mesh) = state.mesh.as_ref() {
        render_mesh(mesh, state.angle, state.tilt);
        draw_text(4, 4, MODELS[state.selected].name, COLOR_CYAN);
        draw_text(4, 122, "Back: Backspace", COLOR_WHITE);
    } else {
        draw_text(4, 40, "Model parse failed", COLOR_WHITE);
        draw_text(4, 52, "Back: Backspace", COLOR_WHITE);
    }
}

fn render_mesh(mesh: &Mesh, angle: f32, tilt: f32) {
    let (sy, cy) = (sinf(angle), cosf(angle));
    let (sp, cp) = (sinf(tilt), cosf(tilt));
    let w = WIDTH as i32;
    let h = HEIGHT as i32;

    for face in mesh.faces.iter() {
        let v0 = transform(mesh, mesh.vertices[face[0]], sy, cy, sp, cp);
        let v1 = transform(mesh, mesh.vertices[face[1]], sy, cy, sp, cp);
        let v2 = transform(mesh, mesh.vertices[face[2]], sy, cy, sp, cp);

        let p0 = project(v0, w, h);
        let p1 = project(v1, w, h);
        let p2 = project(v2, w, h);

        draw_line(p0.0, p0.1, p1.0, p1.1, COLOR_WHITE);
        draw_line(p1.0, p1.1, p2.0, p2.1, COLOR_WHITE);
        draw_line(p2.0, p2.1, p0.0, p0.1, COLOR_WHITE);
    }
}

fn transform(mesh: &Mesh, v: [f32; 3], sy: f32, cy: f32, sp: f32, cp: f32) -> [f32; 3] {
    let mut x = (v[0] - mesh.center[0]) * mesh.scale;
    let mut y = (v[1] - mesh.center[1]) * mesh.scale;
    let mut z = (v[2] - mesh.center[2]) * mesh.scale;

    let xz = x * cy + z * sy;
    let zz = -x * sy + z * cy;
    x = xz;
    z = zz;

    let yz = y * cp - z * sp;
    let zz2 = y * sp + z * cp;
    y = yz;
    z = zz2;

    [x, y, z]
}

fn project(v: [f32; 3], w: i32, h: i32) -> (i32, i32) {
    let depth = v[2] + 3.5;
    let depth = if depth < 0.2 { 0.2 } else { depth };
    let f = 90.0 / depth;
    let sx = (v[0] * f) as i32 + w / 2;
    let sy = (-v[1] * f) as i32 + h / 2;
    (sx, sy)
}

fn parse_stl(bytes: &[u8]) -> Option<Mesh> {
    parse_stl_binary(bytes).or_else(|| parse_stl_ascii(bytes))
}

fn parse_stl_binary(bytes: &[u8]) -> Option<Mesh> {
    if bytes.len() < 84 {
        return None;
    }
    let count = u32::from_le_bytes(bytes[80..84].try_into().ok()?) as usize;
    if bytes.len() < 84 + count * 50 {
        return None;
    }

    let mut vertices = Vec::with_capacity(count * 3);
    let mut faces = Vec::with_capacity(count);
    let mut offset = 84;

    for i in 0..count {
        offset += 12;
        for _ in 0..3 {
            let x = f32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
            let y = f32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?);
            let z = f32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().ok()?);
            vertices.push([x, y, z]);
            offset += 12;
        }
        faces.push([(i * 3) as usize, (i * 3 + 1) as usize, (i * 3 + 2) as usize]);
        offset += 2;
    }

    Some(build_mesh(vertices, faces))
}

fn parse_stl_ascii(bytes: &[u8]) -> Option<Mesh> {
    let text = core::str::from_utf8(bytes).ok()?;
    let mut vertices = Vec::new();
    let mut faces = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("vertex ") {
            continue;
        }
        let mut parts = line.split_whitespace();
        parts.next();
        let x = parts.next()?.parse::<f32>().ok()?;
        let y = parts.next()?.parse::<f32>().ok()?;
        let z = parts.next()?.parse::<f32>().ok()?;
        vertices.push([x, y, z]);
        if vertices.len() % 3 == 0 {
            let i = vertices.len() - 3;
            faces.push([i, i + 1, i + 2]);
        }
    }

    if faces.is_empty() {
        return None;
    }

    Some(build_mesh(vertices, faces))
}

fn build_mesh(vertices: Vec<[f32; 3]>, faces: Vec<[usize; 3]>) -> Mesh {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in vertices.iter() {
        for i in 0..3 {
            if v[i] < min[i] {
                min[i] = v[i];
            }
            if v[i] > max[i] {
                max[i] = v[i];
            }
        }
    }
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let extent = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let max_extent = extent[0].max(extent[1]).max(extent[2]);
    let scale = if max_extent > 0.0 { 1.8 / max_extent } else { 1.0 };

    Mesh {
        vertices,
        faces,
        center,
        scale,
    }
}
