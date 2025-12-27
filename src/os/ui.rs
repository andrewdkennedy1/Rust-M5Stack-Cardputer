use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::keyboard::{CardputerKeyboard, KeyEvent};
use crate::swapchain::DoubleBuffer;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::app::AppContext;
use super::menu::{display_name, menu_path_display, MenuState};
use super::status::StatusSnapshot;

const LIST_TOP: i32 = 48;
const ROW_HEIGHT: i32 = 12;
const MAX_VISIBLE_ROWS: usize = 8;

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

    let title_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    Text::new("Cardputer RustOS", Point::new(2, 10), title_style)
        .draw(fbuf)
        .ok();

    draw_right_aligned(fbuf, &status.clock_text, 10, Rgb565::CSS_CYAN);
    draw_right_aligned(fbuf, &status.wifi_text, 22, Rgb565::CSS_GREEN);
    draw_right_aligned(fbuf, &status.battery_text, 34, Rgb565::CSS_YELLOW);

    let path_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    let path_text = menu_path_display(menu);
    Text::new(&path_text, Point::new(2, 22), path_style)
        .draw(fbuf)
        .ok();

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
        |item| display_name(item),
    );

    let footer_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    let footer = if !context.sd_ready {
        "SD not mounted"
    } else if !context.ota_ready {
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

fn draw_right_aligned(
    target: &mut impl DrawTarget<Color = Rgb565>,
    text: &str,
    y: i32,
    color: Rgb565,
) {
    let style = MonoTextStyle::new(&FONT_6X10, color);
    let width = (text.len() as i32 * 6) + 2;
    let x = (SCREEN_WIDTH as i32 - width).max(0);
    Text::new(text, Point::new(x, y), style).draw(target).ok();
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
)
where
    F: Fn(&T) -> String,
{
    let len = items.len();
    let max_visible = max_visible.min(len.max(1));
    let half = max_visible / 2;
    let mut start = if selected > half {
        selected - half
    } else {
        0
    };
    if len > max_visible {
        start = start.min(len - max_visible);
    } else {
        start = 0;
    }

    if len == 0 {
        let empty_style = MonoTextStyle::new(&FONT_6X10, normal_color);
        Text::new(empty_text, Point::new(left, top), empty_style)
            .draw(target)
            .ok();
    } else {
        for (idx, item) in items.iter().enumerate().skip(start).take(max_visible) {
            let y = top + (idx - start) as i32 * row_height;
            let is_selected = idx == selected;
            let color = if is_selected { selected_color } else { normal_color };
            let style = MonoTextStyle::new(&FONT_6X10, color);
            let prefix = if is_selected { prefix_selected } else { prefix_unselected };
            let line = format!("{}{}", prefix, to_line(item));
            Text::new(&line, Point::new(left, y), style).draw(target).ok();
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

pub fn show_message_and_wait<T: AsRef<str>>(
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
                return;
            }
        }
    }
}
