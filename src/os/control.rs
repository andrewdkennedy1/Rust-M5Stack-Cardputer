use std::path::PathBuf;

use super::menu::MenuAction;

#[derive(Debug, Clone)]
pub enum RemoteCommand {
    Menu(MenuAction),
    FlashBin(PathBuf),
}
