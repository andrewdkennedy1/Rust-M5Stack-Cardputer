use std::f32::consts::PI;
use std::fs::File;
use std::io::Read;

use cardputer::{
    hotkeys,
    keyboard,
    os::{chainload, storage, ui},
    runtime,
    swapchain::OwnedDoubleBuffer,
    SCREEN_HEIGHT, SCREEN_WIDTH,
};
use embedded_gfx::mesh::K3dMesh;
use embedded_gfx::{
    draw::draw,
    mesh::{Geometry, RenderMode},
    perfcounter::PerformanceCounter,
    K3dengine,
};
use embedded_graphics::Drawable;
use embedded_graphics::{
    geometry::Point,
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    text::Text,
};
use embedded_graphics_core::pixelcolor::{Rgb565, WebColors};
use load_stl::embed_stl;
use log::info;
use nalgebra::Point3;

#[no_mangle]
extern "Rust" fn __pender(_context: *mut ()) {}

fn make_xz_plane() -> Vec<[f32; 3]> {
    let step = 1.0;
    let nsteps = 10;

    let mut vertices = Vec::new();
    for i in 0..nsteps {
        for j in 0..nsteps {
            vertices.push([
                (i as f32 - nsteps as f32 / 2.0) * step,
                0.0,
                (j as f32 - nsteps as f32 / 2.0) * step,
            ]);
        }
    }

    vertices
}

// Container to hold the data for Geometry so it lives long enough
struct StlData {
    vertices: Vec<[f32; 3]>,
    faces: Vec<[usize; 3]>,
}

impl StlData {
    fn as_geometry(&self) -> Geometry {
        Geometry {
            vertices: &self.vertices,
            faces: &self.faces,
            colors: &[],
            lines: &[],
            normals: &[],
        }
    }
}

// Simple parsing: Triangle soup (no deduplication for speed/simplicity)
fn parse_stl(bytes: &[u8]) -> Option<StlData> {
    if bytes.len() < 84 { return None; }
    
    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    if bytes.len() < 84 + count * 50 { return None; }

    let mut vertices = Vec::with_capacity(count * 3);
    let mut faces = Vec::with_capacity(count);

    let mut offset = 84;
    for i in 0..count {
        // Skip normal (12 bytes)
        offset += 12;

        for _ in 0..3 {
           let x = f32::from_le_bytes(bytes[offset..offset+4].try_into().unwrap());
           let y = f32::from_le_bytes(bytes[offset+4..offset+8].try_into().unwrap());
           let z = f32::from_le_bytes(bytes[offset+8..offset+12].try_into().unwrap());
           vertices.push([x, y, z]);
           offset += 12;
        }

        faces.push([
            (i * 3) as usize, 
            (i * 3 + 1) as usize, 
            (i * 3 + 2) as usize
        ]);

        // attribute byte count (2 bytes)
        offset += 2;
    }

    Some(StlData { vertices, faces })
}


fn load_stl_from_path(path: &str, default_geometry: Geometry<'static>) -> StlData {
    if let Ok(mut file) = File::open(path) {
        let mut buffer = Vec::new();
        if file.read_to_end(&mut buffer).is_ok() {
            if let Some(data) = parse_stl(&buffer) {
                 info!("Loaded STL from {}", path);
                 return data;
            }
        }
    }
    info!("Using embedded STL for {}", path);
    
    // We need to copy default geometry into Owned StlData
    StlData {
        vertices: default_geometry.vertices.to_vec(),
        faces: default_geometry.faces.to_vec(),
    }
}

fn build_mesh(stl: &StlData) -> K3dMesh {
    let mut mesh = K3dMesh::new(stl.as_geometry());
    mesh.set_render_mode(RenderMode::Lines);
    mesh.set_scale(2.0);
    mesh.set_color(Rgb565::CSS_RED);
    mesh
}

#[allow(clippy::approx_constant)]
fn main() {
    runtime::init();

    let (cardputer, _modem) = runtime::take_cardputer();
    let cardputer::hal::CardputerPeripherals {
        display,
        mut keyboard,
        speaker: _,
    } = cardputer;

    let mut buffers = OwnedDoubleBuffer::<SCREEN_WIDTH, SCREEN_HEIGHT>::new();
    buffers.start_thread(display);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Try mount SD
    let _sd = storage::mount_sd_card();

    info!("creating 3d scene");
    //
    // ----------------- CUT HERE -----------------
    //
    let ground_vertices = make_xz_plane();
    let mut ground = K3dMesh::new(Geometry {
        vertices: &ground_vertices,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
    });
    ground.set_color(Rgb565::new(0, 255, 0));

    let default_geometry = embed_stl!("src/bin/3d objects/Suzanne.stl");
    let stl_entries = storage::list_files_with_extension(storage::SD_MODELS_PATH, "stl");
    let mut stl_index = 0usize;
    let mut current_stl = if stl_entries.is_empty() {
        load_stl_from_path("embedded", default_geometry)
    } else {
        load_stl_from_path(&stl_entries[stl_index].path, default_geometry)
    };
    let mut current_mesh = build_mesh(&current_stl);

    let mut engine = K3dengine::new(SCREEN_WIDTH as _, SCREEN_HEIGHT as _);
    engine.camera.set_position(Point3::new(0.0, 2.0, -2.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));
    engine.camera.set_fovy(PI / 4.0);

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let list_header = format!("STL files ({})", storage::SD_MODELS_PATH);
    let list_hint = "[ / ] to switch".to_string();

    let mut moving_parameter: f32 = 0.0;

    info!("starting main loop");
    let mut player_pos = Point3::new(-10.0, 2.0, 0.0);
    let mut player_dir = 0.0f32;
    let mut player_head = 0.0f32;
    loop {
        let fbuf = buffers.swap_framebuffer();

        let ft = perf.get_frametime();
        let dt = ft as f32 / 1_000_000.0;

        perf.start_of_frame();

        let walking_speed = 5.0 * dt;
        let turning_speed = 0.6 * dt;

        let keys = keyboard.read_keys();
        if hotkeys::action_from_keys(&keys) == Some(hotkeys::SystemAction::ReturnToOs) {
            chainload::reboot_to_factory();
        }
        for key in keys {
            match key {
                keyboard::Key::Semicolon => {
                    player_pos.x += player_dir.cos() * walking_speed;
                    player_pos.z += player_dir.sin() * walking_speed;
                }
                keyboard::Key::Period => {
                    player_pos.x -= player_dir.cos() * walking_speed;
                    player_pos.z -= player_dir.sin() * walking_speed;
                }
                keyboard::Key::Slash => {
                    player_pos.x += (player_dir + PI / 2.0).cos() * walking_speed;
                    player_pos.z += (player_dir + PI / 2.0).sin() * walking_speed;
                }
                keyboard::Key::Comma => {
                    player_pos.x -= (player_dir + PI / 2.0).cos() * walking_speed;
                    player_pos.z -= (player_dir + PI / 2.0).sin() * walking_speed;
                }

                keyboard::Key::D => {
                    player_dir += turning_speed;
                }
                keyboard::Key::A => {
                    player_dir -= turning_speed;
                }

                keyboard::Key::E => {
                    player_head += turning_speed;
                }
                keyboard::Key::S => {
                    player_head -= turning_speed;
                }
                _ => {}
            }
        }


        if let Some((keyboard::KeyEvent::Pressed, key)) = keyboard.read_events() {
            match key {
                keyboard::Key::LeftSquareBracket => {
                    if !stl_entries.is_empty() {
                        stl_index = (stl_index + stl_entries.len() - 1) % stl_entries.len();
                        current_stl = load_stl_from_path(&stl_entries[stl_index].path, default_geometry);
                        current_mesh = build_mesh(&current_stl);
                    }
                }
                keyboard::Key::RightSquareBracket => {
                    if !stl_entries.is_empty() {
                        stl_index = (stl_index + 1) % stl_entries.len();
                        current_stl = load_stl_from_path(&stl_entries[stl_index].path, default_geometry);
                        current_mesh = build_mesh(&current_stl);
                    }
                }
                _ => {}
            }
        }

        engine.camera.set_position(player_pos);

        let lookat = player_pos
            + nalgebra::Vector3::new(player_dir.cos(), player_head.sin(), player_dir.sin());
        engine.camera.set_target(lookat);

        current_mesh.set_attitude(-PI / 2.0, moving_parameter * 1.0, 0.0);
        current_mesh.set_position(0.0, 0.7 + (moving_parameter * 2.0).sin() * 0.2, 2.0);

        perf.add_measurement("setup");

        //fbuf.clear(Rgb565::CSS_BLACK).unwrap(); // 2.2ms

        perf.add_measurement("clear");
        engine.render([&ground, &current_mesh], |p| draw(p, fbuf));

        perf.add_measurement("render");

        Text::new(&list_header, Point::new(4, 4), text_style)
            .draw(fbuf)
            .unwrap();

        ui::draw_selectable_list(
            fbuf,
            &stl_entries,
            stl_index,
            16,
            12,
            6,
            4,
            Rgb565::CSS_WHITE,
            Rgb565::CSS_GREEN,
            "> ",
            "  ",
            "No STL files found",
            |entry| entry.name.clone(),
        );

        Text::new(&list_hint, Point::new(4, 128), text_style)
            .draw(fbuf)
            .unwrap();

        Text::new(perf.get_text(), Point::new(140, 20), text_style)
            .draw(fbuf)
            .unwrap();

        perf.discard_measurement();

        moving_parameter += 0.3 * dt;

        //
        // ----------------- CUT HERE -----------------
        //

        buffers.send_framebuffer();

        perf.add_measurement("draw");

        perf.print();

        //info!("-> {}", perf.get_text());
    }
}
