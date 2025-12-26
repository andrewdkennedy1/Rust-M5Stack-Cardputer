use std::ffi::c_void;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use esp_idf_hal::peripherals;
use esp_idf_svc::sys;

use crate::fs::SdCard;
use crate::hal::cardputer_peripherals;
use crate::keyboard::{CardputerKeyboard, Key, KeyEvent};
use crate::swapchain::DoubleBuffer;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

const ROOT_PATH: &str = "/sdcard";
const DEFAULT_APPS_PATH: &str = "/sdcard/apps";
const LIST_TOP: i32 = 32;
const ROW_HEIGHT: i32 = 12;
const MAX_VISIBLE_ROWS: usize = 8;
const FLASH_CHUNK_SIZE: usize = 4096;
const UI_TICK_MS: u64 = 16;

pub fn run() -> ! {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = peripherals::Peripherals::take().unwrap();
    let cardputer = cardputer_peripherals(
        peripherals.pins,
        peripherals.spi2,
        peripherals.ledc,
        peripherals.i2s0,
    );

    let crate::hal::CardputerPeripherals {
        display,
        mut keyboard,
        speaker: _,
    } = cardputer;

    let mut fb0 = Box::new([0u16; SCREEN_WIDTH * SCREEN_HEIGHT]);
    let mut fb1 = Box::new([0u16; SCREEN_WIDTH * SCREEN_HEIGHT]);
    let mut buffers = DoubleBuffer::<SCREEN_WIDTH, SCREEN_HEIGHT>::new(
        fb0.as_mut_ptr() as *mut c_void,
        fb1.as_mut_ptr() as *mut c_void,
    );
    buffers.start_thread(display);

    render_status(
        &mut buffers,
        "Cardputer Loader",
        &["Mounting SD card..."],
        None,
    );

    let sd = SdCard::new(
        ROOT_PATH,
        sys::spi_host_device_t_SPI3_HOST,
        39,
        14,
        40,
        12,
    )
    .ok();

    let sd_ready = sd.is_some();
    let ota_ready = ota_partition_available();

    let root = PathBuf::from(ROOT_PATH);
    let start = if Path::new(DEFAULT_APPS_PATH).is_dir() {
        PathBuf::from(DEFAULT_APPS_PATH)
    } else {
        root.clone()
    };

    let mut menu = MenuState::new(root, start);
    if sd_ready {
        if let Err(err) = menu.refresh() {
            show_message_and_wait(
                &mut buffers,
                &mut keyboard,
                "SD Error",
                &[format!("Failed to read: {}", err)],
            );
        }
    }

    loop {
        render_menu(&mut buffers, &menu, sd_ready, ota_ready);

        if let Some(action) = read_menu_action(&mut keyboard) {
            match action {
                MenuAction::Up => menu.move_up(),
                MenuAction::Down => menu.move_down(),
                MenuAction::Refresh => {
                    if sd_ready {
                        if let Err(err) = menu.refresh() {
                            show_message_and_wait(
                                &mut buffers,
                                &mut keyboard,
                                "SD Error",
                                &[format!("Failed to read: {}", err)],
                            );
                        }
                    }
                }
                MenuAction::Back => {
                    if menu.go_back() && sd_ready {
                        if let Err(err) = menu.refresh() {
                            show_message_and_wait(
                                &mut buffers,
                                &mut keyboard,
                                "SD Error",
                                &[format!("Failed to read: {}", err)],
                            );
                        }
                    }
                }
                MenuAction::Select => {
                    if let Some(item) = menu.selected_item().cloned() {
                        match item {
                            MenuItem::Back => {
                                if menu.go_back() && sd_ready {
                                    if let Err(err) = menu.refresh() {
                                        show_message_and_wait(
                                            &mut buffers,
                                            &mut keyboard,
                                            "SD Error",
                                            &[format!("Failed to read: {}", err)],
                                        );
                                    }
                                }
                            }
                            MenuItem::Dir(path) => {
                                menu.enter_dir(path);
                                if sd_ready {
                                    if let Err(err) = menu.refresh() {
                                        show_message_and_wait(
                                            &mut buffers,
                                            &mut keyboard,
                                            "SD Error",
                                            &[format!("Failed to read: {}", err)],
                                        );
                                    }
                                }
                            }
                            MenuItem::App(path) => {
                                if !sd_ready {
                                    show_message_and_wait(
                                        &mut buffers,
                                        &mut keyboard,
                                        "SD Missing",
                                        &["Insert SD card and reboot."],
                                    );
                                } else if !ota_ready {
                                    show_message_and_wait(
                                        &mut buffers,
                                        &mut keyboard,
                                        "OTA Missing",
                                        &[
                                            "No OTA partitions found.",
                                            "Update partitions.csv and rebuild.",
                                        ],
                                    );
                                } else if let Err(err) = flash_and_reboot(&mut buffers, &path) {
                                    let lines = err.to_lines();
                                    show_message_and_wait(
                                        &mut buffers,
                                        &mut keyboard,
                                        "Flash Error",
                                        &lines,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        std::thread::sleep(Duration::from_millis(UI_TICK_MS));
    }
}

#[derive(Clone, Debug)]
enum MenuItem {
    Back,
    Dir(PathBuf),
    App(PathBuf),
}

#[derive(Debug)]
struct MenuState {
    root: PathBuf,
    current: PathBuf,
    items: Vec<MenuItem>,
    selected: usize,
}

impl MenuState {
    fn new(root: PathBuf, current: PathBuf) -> Self {
        Self {
            root,
            current,
            items: Vec::new(),
            selected: 0,
        }
    }

    fn refresh(&mut self) -> std::io::Result<()> {
        self.items = read_menu_items(&self.root, &self.current)?;
        self.clamp_selected();
        Ok(())
    }

    fn selected_item(&self) -> Option<&MenuItem> {
        self.items.get(self.selected)
    }

    fn move_up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.items.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.items.len();
    }

    fn go_back(&mut self) -> bool {
        if self.current == self.root {
            return false;
        }
        if let Some(parent) = self.current.parent() {
            self.current = parent.to_path_buf();
            self.selected = 0;
            return true;
        }
        false
    }

    fn enter_dir(&mut self, path: PathBuf) {
        self.current = path;
        self.selected = 0;
    }

    fn clamp_selected(&mut self) {
        if self.items.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.items.len() {
            self.selected = self.items.len() - 1;
        }
    }
}

#[derive(Debug)]
enum MenuAction {
    Up,
    Down,
    Select,
    Back,
    Refresh,
}

fn read_menu_action(keyboard: &mut CardputerKeyboard<'static>) -> Option<MenuAction> {
    if let Some((event, key)) = keyboard.read_events() {
        if matches!(event, KeyEvent::Pressed) {
            return match key {
                Key::Semicolon | Key::W => Some(MenuAction::Up),
                Key::Period | Key::S => Some(MenuAction::Down),
                Key::Enter => Some(MenuAction::Select),
                Key::Backspace | Key::Slash => Some(MenuAction::Back),
                Key::Tab | Key::Fn => Some(MenuAction::Refresh),
                _ => None,
            };
        }
    }
    None
}

fn read_menu_items(root: &Path, current: &Path) -> std::io::Result<Vec<MenuItem>> {
    let mut items = Vec::new();
    if current != root {
        items.push(MenuItem::Back);
    }

    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if name.starts_with('.') {
                continue;
            }
        }

        if path.is_dir() {
            items.push(MenuItem::Dir(path));
        } else if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            if ext.eq_ignore_ascii_case("bin") {
                items.push(MenuItem::App(path));
            }
        }
    }

    items.sort_by(|a, b| {
        let (ka, na) = item_sort_key(a);
        let (kb, nb) = item_sort_key(b);
        ka.cmp(&kb).then_with(|| na.cmp(&nb))
    });

    Ok(items)
}

fn item_sort_key(item: &MenuItem) -> (u8, String) {
    match item {
        MenuItem::Back => (0, String::new()),
        MenuItem::Dir(path) => (1, path_display_name(path)),
        MenuItem::App(path) => (2, path_display_name(path)),
    }
}

fn path_display_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from("?"))
        .to_lowercase()
}

fn display_name(item: &MenuItem) -> String {
    match item {
        MenuItem::Back => "..".to_string(),
        MenuItem::Dir(path) => format!(
            "[{}]",
            path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| String::from("?"))
        ),
        MenuItem::App(path) => path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| String::from("?")),
    }
}

fn render_menu(
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    menu: &MenuState,
    sd_ready: bool,
    ota_ready: bool,
) {
    let fbuf = buffers.swap_framebuffer();
    let _ = fbuf.clear(Rgb565::BLACK);

    let title_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    Text::new("Cardputer Loader", Point::new(2, 10), title_style)
        .draw(fbuf)
        .ok();

    let path_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    let path_text = menu_path_display(menu);
    Text::new(&path_text, Point::new(2, 22), path_style)
        .draw(fbuf)
        .ok();

    let len = menu.items.len();
    let max_visible = MAX_VISIBLE_ROWS.min(len.max(1));
    let half = max_visible / 2;
    let mut start = if menu.selected > half {
        menu.selected - half
    } else {
        0
    };
    if len > max_visible {
        start = start.min(len - max_visible);
    } else {
        start = 0;
    }

    if len == 0 {
        let empty_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
        Text::new("(empty)", Point::new(2, LIST_TOP), empty_style)
            .draw(fbuf)
            .ok();
    } else {
        for (idx, item) in menu.items.iter().enumerate().skip(start).take(max_visible) {
            let y = LIST_TOP + (idx - start) as i32 * ROW_HEIGHT;
            let selected = idx == menu.selected;
            let color = if selected {
                Rgb565::CSS_YELLOW
            } else {
                Rgb565::CSS_WHITE
            };
            let style = MonoTextStyle::new(&FONT_6X10, color);
            let prefix = if selected { "> " } else { "  " };
            let line = format!("{}{}", prefix, display_name(item));
            Text::new(&line, Point::new(2, y), style).draw(fbuf).ok();
        }
    }

    let footer_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    let footer = if !sd_ready {
        "SD not mounted"
    } else if !ota_ready {
        "OTA partitions missing"
    } else {
        "Up/Down: ;/.  Enter: load  Back: Backspace"
    };
    let footer_y = SCREEN_HEIGHT as i32 - 4;
    Text::new(footer, Point::new(2, footer_y), footer_style)
        .draw(fbuf)
        .ok();

    buffers.send_framebuffer();
}

fn menu_path_display(menu: &MenuState) -> String {
    if menu.current == menu.root {
        "/".to_string()
    } else {
        let rel = menu
            .current
            .strip_prefix(&menu.root)
            .unwrap_or(&menu.current);
        format!("/{}", rel.to_string_lossy())
    }
}

fn render_status(
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    title: &str,
    lines: &[&str],
    progress: Option<FlashProgress>,
) {
    let fbuf = buffers.swap_framebuffer();
    let _ = fbuf.clear(Rgb565::BLACK);

    let title_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    Text::new(title, Point::new(2, 10), title_style)
        .draw(fbuf)
        .ok();

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    for (idx, line) in lines.iter().enumerate() {
        let y = 28 + idx as i32 * ROW_HEIGHT;
        Text::new(*line, Point::new(2, y), text_style)
            .draw(fbuf)
            .ok();
    }

    if let Some(progress) = progress {
        render_progress_bar(fbuf, progress);
    }

    buffers.send_framebuffer();
}

fn render_progress_bar(
    target: &mut impl DrawTarget<Color = Rgb565>,
    progress: FlashProgress,
) {
    let bar_width: u32 = 180;
    let bar_height: u32 = 10;
    let bar_x: i32 = 20;
    let bar_y: i32 = 90;

    let outline = Rectangle::new(
        Point::new(bar_x, bar_y),
        Size::new(bar_width, bar_height),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_WHITE, 1));
    outline.draw(target).ok();

    if let Some(total) = progress.total {
        if total > 0 {
            let pct = (progress.written.saturating_mul(100) / total).min(100);
            let filled = (bar_width.saturating_sub(2) as usize * pct / 100) as u32;
            if filled > 0 {
                let fill_rect = Rectangle::new(
                    Point::new(bar_x + 1, bar_y + 1),
                    Size::new(filled, bar_height - 2),
                )
                .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_YELLOW));
                fill_rect.draw(target).ok();
            }

            let text = format!("{}%", pct);
            let style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
            Text::new(
                &text,
                Point::new(bar_x + bar_width as i32 + 6, bar_y + 8),
                style,
            )
                .draw(target)
                .ok();
        }
    }
}

fn show_message_and_wait<T: AsRef<str>>(
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    keyboard: &mut CardputerKeyboard<'static>,
    title: &str,
    lines: &[T],
) {
    let text: Vec<&str> = lines.iter().map(|line| line.as_ref()).collect();
    render_status(buffers, title, &text, None);
    wait_for_keypress(keyboard);
}

fn wait_for_keypress(keyboard: &mut CardputerKeyboard<'static>) {
    loop {
        if let Some((event, _)) = keyboard.read_events() {
            if matches!(event, KeyEvent::Pressed) {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn ota_partition_available() -> bool {
    let current = unsafe { sys::esp_ota_get_running_partition() };
    let update = unsafe { sys::esp_ota_get_next_update_partition(current) };
    !update.is_null()
}

#[derive(Clone, Copy)]
struct FlashProgress {
    written: usize,
    total: Option<usize>,
}

fn flash_and_reboot(
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    path: &Path,
) -> Result<(), FlashError> {
    let mut file = fs::File::open(path).map_err(FlashError::Open)?;
    let total = file.metadata().ok().map(|m| m.len() as usize);

    let current = unsafe { sys::esp_ota_get_running_partition() };
    let update = unsafe { sys::esp_ota_get_next_update_partition(current) };
    if update.is_null() {
        return Err(FlashError::NoOtaPartition);
    }

    let part_size = unsafe { (*update).size as usize };
    if let Some(total) = total {
        if total > part_size {
            return Err(FlashError::FileTooLarge { total, part_size });
        }
    }

    render_status(
        buffers,
        "Flashing",
        &[path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("app.bin")],
        Some(FlashProgress {
            written: 0,
            total,
        }),
    );

    let mut handle: sys::esp_ota_handle_t = 0;
    let err = unsafe { sys::esp_ota_begin(update, sys::OTA_SIZE_UNKNOWN as usize, &mut handle) };
    if err != 0 {
        return Err(FlashError::OtaBegin(err));
    }

    let mut buf = [0u8; FLASH_CHUNK_SIZE];
    let mut written = 0usize;
    let mut last_render = Instant::now();
    let mut last_pct = None::<usize>;

    loop {
        let n = match file.read(&mut buf) {
            Ok(n) => n,
            Err(err) => {
                return Err(FlashError::Read(err));
            }
        };
        if n == 0 {
            break;
        }

        let err = unsafe { sys::esp_ota_write(handle, buf.as_ptr() as *const c_void, n) };
        if err != 0 {
            return Err(FlashError::OtaWrite(err));
        }

        written += n;
        let pct = total.map(|t| (written.saturating_mul(100) / t).min(100));
        let should_render = match (pct, last_pct) {
            (Some(pct), Some(last)) => pct != last,
            (Some(_), None) => true,
            (None, _) => last_render.elapsed() > Duration::from_millis(150),
        };

        if should_render || last_render.elapsed() > Duration::from_millis(150) {
            last_render = Instant::now();
            last_pct = pct;
            render_status(
                buffers,
                "Flashing",
                &[path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("app.bin")],
                Some(FlashProgress {
                    written,
                    total,
                }),
            );
        }
    }

    let err = unsafe { sys::esp_ota_end(handle) };
    if err != 0 {
        return Err(FlashError::OtaEnd(err));
    }

    let err = unsafe { sys::esp_ota_set_boot_partition(update) };
    if err != 0 {
        return Err(FlashError::OtaSetBoot(err));
    }

    render_status(buffers, "Rebooting", &["Switching app..."], None);
    std::thread::sleep(Duration::from_millis(500));
    unsafe { sys::esp_restart() };
    Ok(())
}

#[derive(Debug)]
enum FlashError {
    Open(std::io::Error),
    Read(std::io::Error),
    NoOtaPartition,
    FileTooLarge { total: usize, part_size: usize },
    OtaBegin(i32),
    OtaWrite(i32),
    OtaEnd(i32),
    OtaSetBoot(i32),
}

impl FlashError {
    fn to_lines(&self) -> Vec<String> {
        match self {
            FlashError::Open(err) => vec![format!("Open failed: {}", err)],
            FlashError::Read(err) => vec![format!("Read failed: {}", err)],
            FlashError::NoOtaPartition => vec![
                "No OTA partition available.".to_string(),
                "Check partitions.csv.".to_string(),
            ],
            FlashError::FileTooLarge { total, part_size } => vec![
                format!("File too large: {} bytes", total),
                format!("Partition size: {} bytes", part_size),
            ],
            FlashError::OtaBegin(err) => vec![format!("OTA begin failed: {}", err)],
            FlashError::OtaWrite(err) => vec![format!("OTA write failed: {}", err)],
            FlashError::OtaEnd(err) => vec![format!("OTA end failed: {}", err)],
            FlashError::OtaSetBoot(err) => vec![format!("Set boot failed: {}", err)],
        }
    }
}
