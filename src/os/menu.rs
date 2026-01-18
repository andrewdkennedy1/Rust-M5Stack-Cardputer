use std::fs;
use std::path::{Path, PathBuf};

use crate::keyboard::{CardputerKeyboard, Key, KeyEvent};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MainTab {
    Apps,
    Tools,
    Settings,
}

impl MainTab {
    pub fn next(&self) -> Self {
        match self {
            MainTab::Apps => MainTab::Tools,
            MainTab::Tools => MainTab::Settings,
            MainTab::Settings => MainTab::Apps,
        }
    }
}

#[derive(Clone, Debug)]
pub enum MenuItem {
    Back,
    MountSD,
    UsbMsc,
    Dir(PathBuf),
    App(PathBuf),
    // Tools
    WifiScan,
    BatteryCheck,
    StorageInfo,
    SystemInfo,
    // Settings
    DisplayBrightness,
    DisplayInvert,
    WifiConnect,
    WifiStatus,
    DateTime,
    About,
}

#[derive(Clone, Debug)]
pub struct MenuEntry {
    pub item: MenuItem,
    pub label: String,
    sort_key: (u8, String),
}

#[derive(Debug)]
pub struct MenuState {
    pub active_tab: MainTab,
    pub root: PathBuf,
    pub current: PathBuf,
    pub items: Vec<MenuEntry>,
    pub selected: usize,
    pub path_display: String,
}

impl MenuState {
    pub fn new(root: PathBuf, current: PathBuf) -> Self {
        let mut state = Self {
            active_tab: MainTab::Apps,
            root,
            current,
            items: Vec::new(),
            selected: 0,
            path_display: String::new(),
        };
        state.update_path_display();
        state
    }

    pub fn switch_tab(&mut self) {
        self.active_tab = self.active_tab.next();
        self.selected = 0;
        self.update_path_display();
    }

    pub fn refresh(&mut self) -> std::io::Result<()> {
        match self.active_tab {
            MainTab::Apps => {
                self.items = read_app_items(&self.root, &self.current)?;
            }
            MainTab::Tools => {
                self.items = get_tools_items();
            }
            MainTab::Settings => {
                self.items = get_settings_items();
            }
        }
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
        if self.active_tab == MainTab::Apps {
            if self.current == self.root {
                return false;
            }
            if let Some(parent) = self.current.parent() {
                self.current = parent.to_path_buf();
                self.selected = 0;
                self.update_path_display();
                return true;
            }
        }
        false
    }

    pub fn enter_dir(&mut self, path: PathBuf) {
        if self.active_tab == MainTab::Apps {
            self.current = path;
            self.selected = 0;
            self.update_path_display();
        }
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
        match self.active_tab {
            MainTab::Apps => {
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
            MainTab::Tools => self.path_display = "Tools".to_string(),
            MainTab::Settings => self.path_display = "Settings".to_string(),
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
    TabNext,
}

pub fn read_menu_action(keyboard: &mut CardputerKeyboard<'static>) -> Option<MenuAction> {
    if let Some((event, key)) = keyboard.read_events() {
        if matches!(event, KeyEvent::Pressed) {
            return match key {
                Key::Semicolon | Key::W => Some(MenuAction::Up),
                Key::Period | Key::S => Some(MenuAction::Down),
                Key::Enter => Some(MenuAction::Select),
                Key::Backspace | Key::Slash => Some(MenuAction::Back),
                Key::Fn => Some(MenuAction::Refresh),
                Key::Tab => Some(MenuAction::TabNext),
                _ => None,
            };
        }
    }
    None
}

fn read_app_items(root: &Path, current: &Path) -> std::io::Result<Vec<MenuEntry>> {
    let mut items = Vec::new();
    if current != root {
        items.push(build_entry(MenuItem::Back));
    } else if fs::read_dir(current).is_ok() {
        // Only show USB MSC option at the root of Apps
        items.push(build_entry(MenuItem::UsbMsc));
    }

    // Try to read directory, but don't fail if it doesn't exist (e.g. unmounted SD)
    if let Ok(entries) = fs::read_dir(current) {
        for entry in entries.flatten() {
            let path = entry.path();

            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }

            if path.is_dir() {
                items.push(build_entry(MenuItem::Dir(path)));
            } else if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("bin") {
                    items.push(build_entry(MenuItem::App(path)));
                }
            }
        }
    }

    // If empty at root, show Mount SD
    if items.is_empty() && current == root {
        items.push(build_entry(MenuItem::MountSD));
    }

    items.sort_by(|a, b| {
        let (ka, na) = &a.sort_key;
        let (kb, nb) = &b.sort_key;
        ka.cmp(kb).then_with(|| na.cmp(nb))
    });

    Ok(items)
}

fn get_tools_items() -> Vec<MenuEntry> {
    vec![
        build_entry(MenuItem::WifiScan),
        build_entry(MenuItem::BatteryCheck),
        build_entry(MenuItem::StorageInfo),
        build_entry(MenuItem::SystemInfo),
    ]
}

fn get_settings_items() -> Vec<MenuEntry> {
    vec![
        build_entry(MenuItem::DisplayBrightness),
        build_entry(MenuItem::DisplayInvert),
        build_entry(MenuItem::WifiConnect),
        build_entry(MenuItem::WifiStatus),
        build_entry(MenuItem::DateTime),
        build_entry(MenuItem::About),
    ]
}

fn build_entry(item: MenuItem) -> MenuEntry {
    let (label, sort_name, sort_group) = match &item {
        MenuItem::Back => ("..".to_string(), String::new(), 0),
        MenuItem::MountSD => ("Mount SD Card".to_string(), String::new(), 0),
        MenuItem::UsbMsc => ("Expose via USB".to_string(), String::new(), 0),
        MenuItem::Dir(path) => (format!("[{}]", path_name(path)), path_sort_key(path), 1),
        MenuItem::App(path) => (path_name(path), path_sort_key(path), 3),
        
        // Tools
        MenuItem::WifiScan => ("WiFi Scanner".to_string(), "wifi".to_string(), 5),
        MenuItem::BatteryCheck => ("Battery Check".to_string(), "batt".to_string(), 5),
        MenuItem::StorageInfo => ("Storage Info".to_string(), "store".to_string(), 5),
        MenuItem::SystemInfo => ("System Info".to_string(), "sys".to_string(), 5),

        // Settings
        MenuItem::DisplayBrightness => ("Display Brightness".to_string(), "disp".to_string(), 5),
        MenuItem::DisplayInvert => ("Invert Colors".to_string(), "inv".to_string(), 5),
        MenuItem::WifiConnect => ("Connect WiFi".to_string(), "net".to_string(), 5),
        MenuItem::WifiStatus => ("WiFi Status".to_string(), "stat".to_string(), 5),
        MenuItem::DateTime => ("Date & Time".to_string(), "time".to_string(), 5),
        MenuItem::About => ("About Device".to_string(), "about".to_string(), 5),
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

pub fn menu_path_display(menu: &MenuState) -> &str {
    &menu.path_display
}
