use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::keyboard::{CardputerKeyboard, KeyEvent};
use crate::swapchain::DoubleBuffer;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

use std::time::{Duration, Instant};

use super::app::AppContext;
use super::menu::{menu_path_display, MainTab, MenuState};
use super::status::StatusSnapshot;

const STATUS_BAR_HEIGHT: i32 = 12;
const TAB_BAR_HEIGHT: i32 = 14;
const LIST_TOP: i32 = STATUS_BAR_HEIGHT + TAB_BAR_HEIGHT + 2;
const ROW_HEIGHT: i32 = 12;
const MAX_VISIBLE_ROWS: usize = 7;

#[derive(Clone, Copy, Debug)]
pub struct FlashProgress {
    pub written: usize,
    pub total: Option<usize>,
}

pub fn render_menu(
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    menu: &MenuState,
    context: &AppContext,
    status: &StatusSnapshot,
) {
    let fbuf = buffers.swap_framebuffer();
    let _ = fbuf.clear(Rgb565::BLACK);

    // Draw Status Bar (Condensed)
    draw_status_bar(fbuf, status);

    // Draw Tab Bar
    draw_tabs(fbuf, menu.active_tab);

    // Draw Path if in Apps tab and not at root
    if menu.active_tab == MainTab::Apps {
        let path_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
        let path_text = menu_path_display(menu);
        // Only draw path if it's not root "/" (cleaner look)
        if path_text != "/" {
             Text::new(path_text, Point::new(2, LIST_TOP - 2), path_style)
                .draw(fbuf)
                .ok();
        }
    }

    draw_selectable_list(
        fbuf,
        &menu.items,
        menu.selected,
        LIST_TOP,
        ROW_HEIGHT,
        MAX_VISIBLE_ROWS,
        2,
        Rgb565::CSS_WHITE,
        Rgb565::CSS_YELLOW,
        "> ",
        "  ",
        "(empty)",
        |entry| entry.label.as_str(),
    );

    let footer_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    let footer = if !context.sd_ready {
        "SD not mounted"
    } else if !context.ota_ready {
        "OTA partitions missing"
    } else {
        "Select: Enter  Tab: Switch"
    };
    let footer_y = SCREEN_HEIGHT as i32 - 4;
    Text::new(footer, Point::new(2, footer_y), footer_style)
        .draw(fbuf)
        .ok();

    buffers.send_framebuffer();
}

fn draw_status_bar(target: &mut impl DrawTarget<Color = Rgb565>, status: &StatusSnapshot) {
    // Background bar
    let rect = Rectangle::new(Point::new(0, 0), Size::new(SCREEN_WIDTH as u32, STATUS_BAR_HEIGHT as u32))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::new(4, 8, 4))); // Dark gray-green
    rect.draw(target).ok();

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    
    // Time (Left)
    Text::new(&status.clock_text, Point::new(2, 9), style).draw(target).ok();
    
    // WiFi / Batt (Right)
    // Shorten WiFi text (e.g. just icon/status) if needed, currently using full text
    // We'll concatenate specific info
    let right_text = format!("{}  {}", status.wifi_text, status.battery_text);
    let width = (right_text.len() as i32 * 6);
    let x = (SCREEN_WIDTH as i32 - width - 2).max(0);
    Text::new(&right_text, Point::new(x, 9), style).draw(target).ok();
}

fn draw_tabs(target: &mut impl DrawTarget<Color = Rgb565>, active: MainTab) {
    let tabs = [MainTab::Apps, MainTab::Tools, MainTab::Settings];
    let tab_width = SCREEN_WIDTH as i32 / 3;
    
    for (i, tab) in tabs.iter().enumerate() {
        let x = i as i32 * tab_width;
        let label = match tab {
            MainTab::Apps => "Apps",
            MainTab::Tools => "Tools",
            MainTab::Settings => "Settings",
        };
        
        let is_active = *tab == active;
        let color = if is_active { Rgb565::CSS_CYAN } else { Rgb565::new(10, 20, 10) };
        let text_color = if is_active { Rgb565::BLACK } else { Rgb565::CSS_WHITE };

        // Tab background
        Rectangle::new(Point::new(x, STATUS_BAR_HEIGHT), Size::new(tab_width as u32, TAB_BAR_HEIGHT as u32))
             .into_styled(PrimitiveStyle::with_fill(color))
             .draw(target)
             .ok();
        
        // Tab label
        let text_len = label.len() as i32 * 6;
        let text_x = x + (tab_width - text_len) / 2;
        let style = MonoTextStyle::new(&FONT_6X10, text_color);
        Text::new(label, Point::new(text_x, STATUS_BAR_HEIGHT + 10), style).draw(target).ok();
    }
}

pub fn draw_selectable_list<T, F>(
    target: &mut impl DrawTarget<Color = Rgb565>,
    items: &[T],
    selected: usize,
    top: i32,
    row_height: i32,
    max_visible: usize,
    left: i32,
    normal_color: Rgb565,
    selected_color: Rgb565,
    prefix_selected: &str,
    prefix_unselected: &str,
    empty_text: &str,
    to_line: F,
) where
    F: Fn(&T) -> &str,
{
    let len = items.len();
    let max_visible = max_visible.min(len.max(1));
    let half = max_visible / 2;
    let mut start = if selected > half { selected - half } else { 0 };
    if len > max_visible {
        start = start.min(len - max_visible);
    } else {
        start = 0;
    }

    let prefix_width = (prefix_selected.len().max(prefix_unselected.len()) as i32) * 6;

    if len == 0 {
        let empty_style = MonoTextStyle::new(&FONT_6X10, normal_color);
        Text::new(empty_text, Point::new(left, top + row_height), empty_style)
            .draw(target)
            .ok();
    } else {
        for (idx, item) in items.iter().enumerate().skip(start).take(max_visible) {
            let y = top + (idx - start) as i32 * row_height + 8; // Offset for text baseline
            let is_selected = idx == selected;
            let color = if is_selected {
                selected_color
            } else {
                normal_color
            };
            let style = MonoTextStyle::new(&FONT_6X10, color);
            let prefix = if is_selected {
                prefix_selected
            } else {
                prefix_unselected
            };
            Text::new(prefix, Point::new(left, y), style)
                .draw(target)
                .ok();
            Text::new(to_line(item), Point::new(left + prefix_width, y), style)
                .draw(target)
                .ok();
        }
    }
}

pub fn render_status<T: AsRef<str>>(
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    title: &str,
    lines: &[T],
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
        Text::new(line.as_ref(), Point::new(2, y), text_style)
            .draw(fbuf)
            .ok();
    }

    if let Some(progress) = progress {
        render_progress_bar(fbuf, progress);
    }

    buffers.send_framebuffer();
}

pub fn render_boot_animation(buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>) {
    let start = Instant::now();
    let duration = Duration::from_millis(900);
    // Precomputed ring offsets to avoid trig on boot.
    let ring: [(i32, i32); 8] = [
        (0, -18),
        (12, -12),
        (18, 0),
        (12, 12),
        (0, 18),
        (-12, 12),
        (-18, 0),
        (-12, -12),
        (0, -18), // Wrap around for safety
    ];
    let ring = &ring[0..8];
    
    let center_x = (SCREEN_WIDTH as i32) / 2;
    let center_y = (SCREEN_HEIGHT as i32) / 2 - 6;

    while start.elapsed() < duration {
        let elapsed = start.elapsed();
        let phase = ((el
