pub mod app;
pub mod chainload;
pub mod menu;
pub mod status;
pub mod storage;
pub mod ui;
pub mod web;

use std::path::PathBuf;
use std::time::Duration;

use esp_idf_svc::sys;

use crate::runtime;
use crate::swapchain::{DoubleBuffer, OwnedDoubleBuffer};
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};
use app::{AppContext, AppLaunch};
use chainload::ota_partition_available;
use menu::{MenuAction, MenuItem, MenuState};
use status::{BatteryGauge, StatusProvider};
use ui::{render_menu, render_status, show_message_and_wait};
use web::start_wifi_file_server;
use storage::{mount_sd_card, SD_APPS_PATH, SD_ROOT};

const UI_TICK_MS: u64 = 16;

fn refresh_menu_or_warn(
    menu: &mut MenuState,
    sd_ready: bool,
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    keyboard: &mut crate::keyboard::CardputerKeyboard<'static>,
) {
    if sd_ready {
        if let Err(err) = menu.refresh() {
            show_message_and_wait(
                buffers,
                keyboard,
                "SD Error",
                &[format!("Failed to read: {}", err)],
            );
        }
    }
}

/// Boot entry point for Cardputer-RustOS.
pub fn boot() -> ! {
    runtime::init();
    unsafe {
        let partition = sys::esp_ota_get_running_partition();
        if !partition.is_null() && (*partition).type_ == sys::esp_partition_type_t_ESP_PARTITION_TYPE_APP && (*partition).subtype == sys::esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_APP_FACTORY {
            sys::esp_ota_mark_app_valid_cancel_rollback();
        }
    }

    let (cardputer, modem) = runtime::take_cardputer();

    let crate::hal::CardputerPeripherals {
        display,
        mut keyboard,
        speaker: _,
    } = cardputer;

    let mut buffers = OwnedDoubleBuffer::<SCREEN_WIDTH, SCREEN_HEIGHT>::new();
    buffers.start_thread(display);

    render_status(
        &mut buffers,
        "Cardputer RustOS",
        &["Mounting SD card..."],
        None,
    );

    let sd = mount_sd_card();

    let sd_ready = sd.is_some();
    let ota_ready = ota_partition_available();

    let wifi_state = start_wifi_file_server(modem, if sd_ready {
        Some(PathBuf::from(SD_ROOT))
    } else {
        None
    });
    let status_provider = StatusProvider::new(wifi_state, BatteryGauge::new());

    let root = PathBuf::from(SD_ROOT);
    let start = if std::path::Path::new(SD_APPS_PATH).is_dir() {
        PathBuf::from(SD_APPS_PATH)
    } else {
        root.clone()
    };

    let mut menu = MenuState::new(root, start);
    refresh_menu_or_warn(&mut menu, sd_ready, &mut buffers, &mut keyboard);

    let context = AppContext::new(sd_ready, ota_ready);

    loop {
        let status = status_provider.snapshot();
        render_menu(&mut buffers, &menu, &context, &status);

        if let Some(action) = menu::read_menu_action(&mut keyboard) {
            match action {
                MenuAction::Up => menu.move_up(),
                MenuAction::Down => menu.move_down(),
                MenuAction::Refresh => {
                    refresh_menu_or_warn(&mut menu, sd_ready, &mut buffers, &mut keyboard);
                }
                MenuAction::Back => {
                    if menu.go_back() {
                        refresh_menu_or_warn(&mut menu, sd_ready, &mut buffers, &mut keyboard);
                    }
                }
                MenuAction::Select => {
                    if let Some(item) = menu.selected_item().cloned() {
                        match item {
                            MenuItem::Back => {
                                if menu.go_back() {
                                    refresh_menu_or_warn(&mut menu, sd_ready, &mut buffers, &mut keyboard);
                                }
                            }
                            MenuItem::Dir(path) => {
                                menu.enter_dir(path);
                                refresh_menu_or_warn(&mut menu, sd_ready, &mut buffers, &mut keyboard);
                            }
                            MenuItem::App(path) => {
                                let launch = AppLaunch::from_path(path);
                                if let Err(err) = context.validate_launch(&launch) {
                                    show_message_and_wait(
                                        &mut buffers,
                                        &mut keyboard,
                                        "Launch Error",
                                        &err.to_lines(),
                                    );
                                } else if let Err(err) = chainload::flash_and_reboot(&mut buffers, &launch.path) {
                                    show_message_and_wait(
                                        &mut buffers,
                                        &mut keyboard,
                                        "Flash Error",
                                        &err.to_lines(),
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
