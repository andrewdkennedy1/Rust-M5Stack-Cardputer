use crate::keyboard::{CardputerKeyboard, Key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAction {
    ReturnToOs,
}

pub fn action_from_keys(keys: &[Key]) -> Option<SystemAction> {
    let mut has_ctrl = false;
    let mut has_backspace = false;

    for key in keys {
        match key {
            Key::Ctrl => has_ctrl = true,
            Key::Backspace => has_backspace = true,
            _ => {}
        }
    }

    if has_ctrl && has_backspace {
        Some(SystemAction::ReturnToOs)
    } else {
        None
    }
}

pub fn poll_action(keyboard: &mut CardputerKeyboard<'_>) -> Option<SystemAction> {
    let keys = keyboard.read_keys();
    action_from_keys(&keys)
}
