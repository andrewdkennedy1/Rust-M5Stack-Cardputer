use std::fs;
use std::path::{Path, PathBuf};

use crate::keyboard::{CardputerKeyboard, Key, KeyEvent};

use super::live_apps::LiveAppKind;

#[derive(Clone, Debug)]
pub enum MenuItem {
    Back,
    Dir(PathBuf),
    App(PathBuf),
    LiveApp(LiveAppKind, PathBuf),
}

#[derive(Clone, Debug)]
pub struct MenuEntry {
    pub item: MenuItem,
    pub label: String,
    sort_key: (u8, String),
}

#[derive(Debug)]
pub struct MenuState {
    pub root: PathBuf,
    pub current: PathBuf,
    pub items: Vec<MenuEntry>,
    pub selected: usize,
    pub path_display: String,
}

impl MenuState {
    pub fn new(root: PathBuf, current: PathBuf) -> Self {
        let mut state = Self {
            root,
            current,
            items: Vec::new(),
            selected: 0,
            path_display: String::new(),
        };
        state.update_path_display();
        state
    }

    pub fn refresh(&mut self) -> std::io::Result<()> {
        self.items = read_menu_items(&self.root, &self.current)?;
        self.update_path_display();
        self.clamp_selected();
        Ok(())
    }

    pub fn selected_item(&self) -> Option<&MenuItem> {
        self.items.get(self.selected).map(|entry| &entry.item)
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
            self.update_path_display();
            return true;
        }
        false
    }

    pub fn enter_dir(&mut self, path: PathBuf) {
        self.current = path;
        self.selected = 0;
        self.update_path_display();
    }

    pub fn release_memory(&mut self) {
        self.items.clear();
        self.items.shrink_to_fit();
        self.selected = 0;
    }

    fn clamp_selected(&mut self) {
        if self.items.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.items.len() {
            self.selected = self.items.len() - 1;
        }
    }

    fn update_path_display(&mut self) {
        if self.current == self.root {
            self.path_display = "/".to_string();
        } else {
            let rel = self
                .current
                .strip_prefix(&self.root)
                .unwrap_or(&self.current);
            self.path_display = format!("/{}", rel.to_string_lossy());
        }
    }
}

#[derive(Debug, Clone)]
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

fn read_menu_items(root: &Path, current: &Path) -> std::io::Result<Vec<MenuEntry>> {
    let mut items = Vec::new();
    if current != root {
        items.push(build_entry(MenuItem::Back));
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
            items.push(build_entry(MenuItem::Dir(path)));
        } else if let Some(kind) = live_app_kind_for_path(&path) {
            items.push(build_entry(MenuItem::LiveApp(kind, path)));
        } else if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            if ext.eq_ignore_ascii_case("bin") {
                items.push(build_entry(MenuItem::App(path)));
            }
        }
    }

    items.sort_by(|a, b| {
        let (ka, na) = &a.sort_key;
        let (kb, nb) = &b.sort_key;
        ka.cmp(kb).then_with(|| na.cmp(nb))
    });

    Ok(items)
}

fn build_entry(item: MenuItem) -> MenuEntry {
    let (label, sort_name, sort_group) = match &item {
        MenuItem::Back => ("..".to_string(), String::new(), 0),
        MenuItem::Dir(path) => (format!("[{}]", path_name(path)), path_sort_key(path), 1),
        MenuItem::LiveApp(_, path) => (path_name(path), path_sort_key(path), 2),
        MenuItem::App(path) => (path_name(path), path_sort_key(path), 3),
    };

    MenuEntry {
        item,
        label,
        sort_key: (sort_group, sort_name),
    }
}

fn path_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from("?"))
}

fn path_sort_key(path: &Path) -> String {
    path_name(path).to_lowercase()
}

fn live_app_kind_for_path(path: &Path) -> Option<LiveAppKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "wasm" => Some(LiveAppKind::Wasm),
        "py" | "mpy" => Some(LiveAppKind::Python),
        _ => None,
    }
}

pub fn menu_path_display(menu: &MenuState) -> &str {
    &menu.path_display
}
