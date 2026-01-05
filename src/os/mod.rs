pub mod app;
pub mod chainload;
pub mod control;
pub mod live_apps;
pub mod menu;
pub mod python_runner;
pub mod status;
pub mod storage;
pub mod ui;
pub mod usb_msc;
pub mod web;

use std::path::PathBuf;
use std::time::Duration;

use esp_idf_svc::sys;


use crate::runtime;
use crate::swapchain::{DoubleBuffer, OwnedDoubleBuffer};
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};
use app::{AppContext, AppLaunch};
use chainload::ota_partition_available;
use control::RemoteCommand;
use live_apps::{LiveAppKind, LiveAppOutcome, LiveAppRunner};
use menu::{MenuAction, MenuItem, MenuState};
use status::{BatteryGauge, StatusProvider};
use ui::{render_menu, render_status, show_message_and_wait};
use storage::{mount_sd_card, SD_APPS_PATH, SD_ROOT};

const UI_TICK_MS: u64 = 16;

fn refresh_menu_or_warn(
    menu: &mut MenuState,
    _sd_ready: bool,
    usb_active: bool,
    warn_on_usb: bool,
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    keyboard: &mut crate::keyboard::CardputerKeyboard<'static>,
) {
    if usb_active {
        if warn_on_usb {
            warn_usb_storage_active(buffers, keyboard);
        }
        return;
    }
    if let Err(err) = menu.refresh() {
        show_message_and_wait(
            buffers,
            keyboard,
            "SD Error",
            &[format!("Failed to read: {}", err)],
        );
    }
}

fn launch_or_report(
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    keyboard: &mut crate::keyboard::CardputerKeyboard<'static>,
    context: &AppContext,
    path: PathBuf,
) {
    let launch = AppLaunch::from_path(path);
    if let Err(err) = context.validate_launch(&launch) {
        show_message_and_wait(buffers, keyboard, "Launch Error", &err.to_lines());
    } else if let Err(err) = chainload::flash_and_reboot(buffers, &launch.path) {
        show_message_and_wait(buffers, keyboard, "Flash Error", &err.to_lines());
    }
}

fn menu_action_to_key(action: MenuAction) -> Option<crate::keyboard::Key> {
    use crate::keyboard::Key;
    match action {
        MenuAction::Up => Some(Key::Semicolon),
        MenuAction::Down => Some(Key::Period),
        MenuAction::Select => Some(Key::Enter),
        MenuAction::Back => Some(Key::Backspace),
        MenuAction::Refresh => Some(Key::Tab),
    }
}

fn warn_usb_storage_active(
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    keyboard: &mut crate::keyboard::CardputerKeyboard<'static>,
) {
    show_message_and_wait(
        buffers,
        keyboard,
        "USB Storage",
        &[
            "SD card is exposed over USB.",
            "Eject USB drive to continue.",
        ],
    );
}

struct WebPause {
    paused: bool,
    live: bool,
    usb: bool,
}

impl WebPause {
    fn new() -> Self {
        Self {
            paused: false,
            live: false,
            usb: false,
        }
    }

    fn set_live(&mut self, web: &web::WebHandle, paused: bool) {
        self.live = paused;
        self.sync(web);
    }

    fn set_usb(&mut self, web: &web::WebHandle, paused: bool) {
        self.usb = paused;
        self.sync(web);
    }

    fn sync(&mut self, web: &web::WebHandle) {
        let should_pause = self.live || self.usb;
        if should_pause && !self.paused {
            web.pause();
            self.paused = true;
        } else if !should_pause && self.paused {
            web.resume();
            self.paused = false;
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
        speaker,
    } = cardputer;

    let mut speaker = Some(speaker);

    let mut buffers = OwnedDoubleBuffer::<SCREEN_WIDTH, SCREEN_HEIGHT>::new();
    buffers.start_thread(display);

    render_status(
        &mut buffers,
        "Cardputer RustOS",
        &["Starting..."],
        None,
    );

    let mut sd: Option<crate::fs::SdCard> = None;
    let mut sd_ready = false;
    let ota_ready = ota_partition_available();
    let mut usb_msc: Option<usb_msc::UsbMsc> = None;

    let (control_tx, control_rx) = std::sync::mpsc::channel();
    let web_handle = web::start_wifi_file_server(
        modem,
        None,
        control_tx.clone(),
    );
    let mut status_provider = StatusProvider::new(web_handle.wifi_state(), BatteryGauge::new());

    let root = PathBuf::from(SD_ROOT);
    let mut menu = MenuState::new(root.clone(), root.clone());
    let mut menu_needs_refresh = true;

    // Serial Command Listener
    let serial_tx = control_tx.clone();
    std::thread::spawn(move || {
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        for line in stdin.lock().lines().flatten() {
            let cmd = match line.trim() {
                "RECOVERY" => RemoteCommand::FlashBin(PathBuf::from("__FACTORY__")), // Special marker for factory reset
                "REBOOT" => RemoteCommand::FlashBin(PathBuf::from("__REBOOT__")),   // Special marker for reboot
                _ => continue,
            };
            let _ = serial_tx.send(cmd);
        }
    });

    let mut context = AppContext::new(sd_ready, ota_ready);
    let mut live_app: Option<LiveAppRunner> = None;
    let mut web_pause = WebPause::new();

    loop {
        if let Some(usb) = usb_msc.as_mut() {
            if let Some(active) = usb.poll() {
                web_pause.set_usb(&web_handle, active);
                menu.release_memory();
                menu_needs_refresh = true;
            }
        }
        let usb_active = usb_msc
            .as_ref()
            .map(|msc| msc.host_active())
            .unwrap_or(false);

        if live_app.is_some() {
            web_pause.set_live(&web_handle, true);
            let mut injected_key = None;
            if let Some(command) = control_rx.try_recv().ok() {
                match command {
                    RemoteCommand::Menu(action) => {
                        if matches!(action, MenuAction::Back) {
                            if let Some(app) = live_app.take() {
                                speaker = app.teardown();
                            }
                            menu_needs_refresh = true;
                        } else if let Some(key) = menu_action_to_key(action) {
                            injected_key = Some((crate::keyboard::KeyEvent::Pressed, key));
                        }
                    }
                    RemoteCommand::RunLive(kind, path) => {
                        if usb_active {
                            warn_usb_storage_active(&mut buffers, &mut keyboard);
                        } else if matches!(kind, LiveAppKind::Python) {
                            python_runner::launch_python_runner(&mut buffers, &mut keyboard, &path);
                            live_app = None;
                            web_pause.set_live(&web_handle, false);
                            menu_needs_refresh = true;
                        } else {
                            menu.release_memory();
                            menu_needs_refresh = true;
                            if let Some(s) = speaker.take() {
                                match LiveAppRunner::load(kind, path, s) {
                                    Ok(new_app) => {
                                        live_app = Some(new_app);
                                    }
                                    Err(err) => {
                                        show_message_and_wait(
                                            &mut buffers,
                                            &mut keyboard,
                                            "App Error",
                                            &[format!("{:?}", err)],
                                        );
                                        // TODO put speaker back? 
                                        // match err { LiveAppError::LoadFailed(_) => ... }
                                        live_app = None;
                                        web_pause.set_live(&web_handle, false);
                                        menu_needs_refresh = true;
                                    }
                                }
                            }
                        }
                    }
                    RemoteCommand::FlashBin(path) => {
                        if usb_active {
                            warn_usb_storage_active(&mut buffers, &mut keyboard);
                        } else {
                            launch_or_report(&mut buffers, &mut keyboard, &context, path);
                        }
                    }
                }
            }

            if let Some(app) = live_app.as_mut() {
                match app.tick(&mut buffers, &mut keyboard, injected_key) {
                    Ok(LiveAppOutcome::Continue) => {}
                    Ok(LiveAppOutcome::Exit) => {
                        if let Some(app) = live_app.take() {
                            speaker = app.teardown();
                        }
                        menu_needs_refresh = true;
                    }
                    Err(err) => {
                        show_message_and_wait(
                            &mut buffers,
                            &mut keyboard,
                            "App Error",
                            &[format!("{:?}", err)],
                        );
                        if let Some(app) = live_app.take() {
                            speaker = app.teardown();
                        }
                        web_pause.set_live(&web_handle, false);
                        menu_needs_refresh = true;
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(UI_TICK_MS));
            continue;
        }

        web_pause.set_live(&web_handle, false);
        if menu_needs_refresh {
            refresh_menu_or_warn(
                &mut menu,
                sd_ready,
                usb_active,
                false,
                &mut buffers,
                &mut keyboard,
            );
            menu_needs_refresh = false;
        }
        let status = status_provider.snapshot();
        render_menu(&mut buffers, &menu, &context, status);

        let command = control_rx
            .try_recv()
            .ok()
            .or_else(|| menu::read_menu_action(&mut keyboard).map(RemoteCommand::Menu));
        if let Some(command) = command {
            match command {
                RemoteCommand::Menu(action) => match action {
                    MenuAction::Up => menu.move_up(),
                    MenuAction::Down => menu.move_down(),
                    MenuAction::Refresh => {
                        refresh_menu_or_warn(
                            &mut menu,
                            sd_ready,
                            usb_active,
                            true,
                            &mut buffers,
                            &mut keyboard,
                        );
                    }
                    MenuAction::Back => {
                        if usb_active {
                            warn_usb_storage_active(&mut buffers, &mut keyboard);
                        } else if menu.go_back() {
                            refresh_menu_or_warn(
                                &mut menu,
                                sd_ready,
                                usb_active,
                                true,
                                &mut buffers,
                                &mut keyboard,
                            );
                        }
                    }
                    MenuAction::Select => {
                        if let Some(item) = menu.selected_item().cloned() {
                            match item {
                                MenuItem::Back => {
                                    if usb_active {
                                        warn_usb_storage_active(&mut buffers, &mut keyboard);
                                    } else if menu.go_back() {
                                        refresh_menu_or_warn(
                                            &mut menu,
                                            sd_ready,
                                            usb_active,
                                            true,
                                            &mut buffers,
                                            &mut keyboard,
                                        );
                                    }
                                }
                                MenuItem::MountSD => {
                                    render_status(&mut buffers, "Mounting", &["Mounting SD card..."], None);
                                    if let Some(card) = mount_sd_card() {
                                        sd = Some(card);
                                        sd_ready = true;
                                        context.sd_ready = true;
                                        web_handle.set_sd_root(PathBuf::from(SD_ROOT));
                                        if std::path::Path::new(SD_APPS_PATH).is_dir() {
                                            menu.enter_dir(PathBuf::from(SD_APPS_PATH));
                                        }
                                        menu_needs_refresh = true;
                                    } else {
                                        show_message_and_wait(
                                            &mut buffers,
                                            &mut keyboard,
                                            "Mount Failed",
                                            &["Check SD card", "and try again."],
                                        );
                                    }
                                }
                                MenuItem::UsbMsc => {
                                    if let Some(ref card) = sd {
                                        show_message_and_wait(
                                            &mut buffers,
                                            &mut keyboard,
                                            "USB Storage",
                                            &[
                                                "USB driver will block",
                                                "the serial port!",
                                                "Press any key to",
                                                "start exposing SD...",
                                            ],
                                        );
                                        usb_msc = usb_msc::UsbMsc::init(card).ok();
                                        web_pause.sync(&web_handle);
                                        menu_needs_refresh = true;
                                    }
                                }
                                MenuItem::Dir(path) => {
                                    if usb_active {
                                        warn_usb_storage_active(&mut buffers, &mut keyboard);
                                    } else {
                                        menu.enter_dir(path);
                                        refresh_menu_or_warn(
                                            &mut menu,
                                            sd_ready,
                                            usb_active,
                                            true,
                                            &mut buffers,
                                            &mut keyboard,
                                        );
                                    }
                                }
                                MenuItem::App(path) => {
                                    if usb_active {
                                        warn_usb_storage_active(&mut buffers, &mut keyboard);
                                    } else {
                                        launch_or_report(
                                            &mut buffers,
                                            &mut keyboard,
                                            &context,
                                            path,
                                        );
                                    }
                                }
                                MenuItem::LiveApp(kind, path) => {
                                    if usb_active {
                                        warn_usb_storage_active(&mut buffers, &mut keyboard);
                                    } else {
                                        menu.release_memory();
                                        menu_needs_refresh = true;
                                        web_pause.set_live(&web_handle, true);
                                        if matches!(kind, LiveAppKind::Python) {
                                            python_runner::launch_python_runner(
                                                &mut buffers,
                                                &mut keyboard,
                                                &path,
                                            );
                                            web_pause.set_live(&web_handle, false);
                                            menu_needs_refresh = true;
                                        } else if let Some(s) = speaker.take() {
                                            match LiveAppRunner::load(kind, path, s) {
                                                Ok(app) => {
                                                    live_app = Some(app);
                                                }
                                                Err(err) => {
                                                    show_message_and_wait(
                                                        &mut buffers,
                                                        &mut keyboard,
                                                        "App Error",
                                                        &[format!("{:?}", err)],
                                                    );
                                                    web_pause.set_live(&web_handle, false);
                                                    menu_needs_refresh = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                RemoteCommand::RunLive(kind, path) => {
                    if usb_active {
                        warn_usb_storage_active(&mut buffers, &mut keyboard);
                    } else if matches!(kind, LiveAppKind::Python) {
                        web_pause.set_live(&web_handle, true);
                        python_runner::launch_python_runner(&mut buffers, &mut keyboard, &path);
                        web_pause.set_live(&web_handle, false);
                        menu_needs_refresh = true;
                    } else {
                        menu.release_memory();
                        menu_needs_refresh = true;
                        web_pause.set_live(&web_handle, true);
                        if let Some(s) = speaker.take() {
                            match LiveAppRunner::load(kind, path, s) {
                                Ok(app) => {
                                    live_app = Some(app);
                                }
                                Err(err) => {
                                    show_message_and_wait(
                                        &mut buffers,
                                        &mut keyboard,
                                        "App Error",
                                        &[format!("{:?}", err)],
                                    );
                                    web_pause.set_live(&web_handle, false);
                                    menu_needs_refresh = true;
                                }
                            }
                        }
                    }
                }
                RemoteCommand::FlashBin(path) => {
                    if path.to_str() == Some("__FACTORY__") {
                        chainload::reboot_to_factory();
                    } else if path.to_str() == Some("__REBOOT__") {
                        unsafe { sys::esp_restart(); }
                    } else if usb_active {
                        warn_usb_storage_active(&mut buffers, &mut keyboard);
                    } else {
                        launch_or_report(&mut buffers, &mut keyboard, &context, path);
                    }
                }
            }
        }

        std::thread::sleep(Duration::from_millis(UI_TICK_MS));
    }
}
