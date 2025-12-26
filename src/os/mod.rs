pub mod app;
pub mod chainload;
pub mod menu;
pub mod status;
pub mod ui;
pub mod web;

use std::ffi::c_void;
use std::path::PathBuf;
use std::time::Duration;

use esp_idf_hal::peripherals;
use esp_idf_svc::sys;

use crate::fs::SdCard;
use crate::hal::cardputer_peripherals;
use crate::swapchain::DoubleBuffer;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};
use app::{AppContext, AppLaunch};
use chainload::ota_partition_available;
use menu::{MenuAction, MenuItem, MenuState};
use status::{BatteryGauge, StatusProvider};
use ui::{render_menu, render_status, show_message_and_wait};
use web::start_wifi_file_server;

const ROOT_PATH: &str = "/sdcard";
const DEFAULT_APPS_PATH: &str = "/sdcard/apps";
const UI_TICK_MS: u64 = 16;

/// Boot entry point for Cardputer-RustOS.
pub fn boot() -> ! {
    sys::link_patches();
    unsafe {
        let partition = sys::esp_ota_get_running_partition();
        if !partition.is_null() && (*partition).type_ == sys::esp_partition_type_t_ESP_PARTITION_TYPE_APP {
            sys::esp_ota_mark_app_valid_cancel_rollback();
        }
    }
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = peripherals::Peripherals::take().unwrap();
    let peripherals::Peripherals {
        pins,
        spi2,
        ledc,
        i2s0,
        modem,
        ..
    } = peripherals;
    let cardputer = cardputer_peripherals(pins, spi2, ledc, i2s0);

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
        "Cardputer RustOS",
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

    let wifi_state = start_wifi_file_server(modem, if sd_ready {
        Some(PathBuf::from(ROOT_PATH))
    } else {
        None
    });
    let status_provider = StatusProvider::new(wifi_state, BatteryGauge::new());

    let root = PathBuf::from(ROOT_PATH);
    let start = if std::path::Path::new(DEFAULT_APPS_PATH).is_dir() {
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

    let context = AppContext::new(sd_ready, ota_ready);

    loop {
        let status = status_provider.snapshot();
        render_menu(&mut buffers, &menu, &context, &status);

        if let Some(action) = menu::read_menu_action(&mut keyboard) {
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
