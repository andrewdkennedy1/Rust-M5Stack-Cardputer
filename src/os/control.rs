use std::path::PathBuf;

use super::live_apps::LiveAppKind;
use super::menu::MenuAction;

#[derive(Debug, Clone)]
pub enum RemoteCommand {
    Menu(MenuAction),
    FlashBin(PathBuf),
    RunLive(LiveAppKind, PathBuf),
}
