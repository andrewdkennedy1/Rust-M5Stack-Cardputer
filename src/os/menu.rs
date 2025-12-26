use std::fs;
use std::path::{Path, PathBuf};

use crate::keyboard::{CardputerKeyboard, Key, KeyEvent};

#[derive(Clone, Debug)]
pub enum MenuItem {
    Back,
    Dir(PathBuf),
    App(PathBuf),
}

#[derive(Debug)]
pub struct MenuState {
    pub root: PathBuf,
    pub current: PathBuf,
    pub items: Vec<MenuItem>,
    pub selected: usize,
}

impl MenuState {
    pub fn new(root: PathBuf, current: PathBuf) -> Self {
        Self {
            root,
            current,
            items: Vec::new(),
            selected: 0,
        }
    }

    pub fn refresh(&mut self) -> std::io::Result<()> {
        self.items = read_menu_items(&self.root, &self.current)?;
        self.clamp_selected();
        Ok(())
    }

    pub fn selected_item(&self) -> Option<&MenuItem> {
        self.items.get(self.selected)
    }

    pub fn move_up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.items.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.items.len();
    }

    pub fn go_back(&mut self) -> bool {
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

    pub fn enter_dir(&mut self, path: PathBuf) {
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
pub enum MenuAction {
    Up,
    Down,
    Select,
    Back,
    Refresh,
}

pub fn read_menu_action(keyboard: &mut CardputerKeyboard<'static>) -> Option<MenuAction> {
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

pub fn display_name(item: &MenuItem) -> String {
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

pub fn menu_path_display(menu: &MenuState) -> String {
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
