use std::fs::File;
use std::io::{Read, Write as StdWrite};
use std::path::PathBuf;
use std::sync::{mpsc::Sender, Arc, Mutex};
use std::thread;
use std::time::Duration;

use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write as HttpWrite;
use esp_idf_svc::nvs::EspDefaultNvsPartition;

use super::chainload;
use super::control::RemoteCommand;
use super::live_apps::LiveAppKind;
use super::menu::MenuAction;
use crate::swapchain::ScreenCaptureHandle;
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use log::{error, info};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub enum WifiMode {
    AccessPoint,
    Station,
}

#[derive(Clone, Debug)]
pub struct WifiState {
    pub mode: WifiMode,
    pub ssid: String,
    pub ip: Option<String>,
}

pub type WifiStateHandle = Arc<Mutex<WifiState>>;

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct WifiConfig {
    ssid: String,
    password: String,
    auto_connect: bool,
}

#[derive(Serialize, Debug)]
struct FileEntry {
    name: String,
    is_dir: bool,
    size: u64,
}

/// Returns a placeholder wifi state for when WiFi is disabled.
pub fn wifi_disabled_state() -> WifiStateHandle {
    Arc::new(Mutex::new(WifiState {
        mode: WifiMode::Station,
        ssid: "WiFi off".to_string(),
        ip: None,
    }))
}

pub fn start_wifi_file_server(
    modem: Modem,
    sd_root: Option<PathBuf>,
    screen_capture: ScreenCaptureHandle,
    control_tx: Sender<RemoteCommand>,
) -> WifiStateHandle {
    let state = Arc::new(Mutex::new(WifiState {
        mode: WifiMode::Station,
        ssid: "Checking SD...".to_string(),
        ip: None,
    }));

    const WIFI_THREAD_STACK_BYTES: usize = 16 * 1024;

    let state_thread = state.clone();
    let spawn_result = thread::Builder::new()
        .stack_size(WIFI_THREAD_STACK_BYTES)
        .spawn(move || {
            if let Err(err) = bringup_wifi_and_server(modem, sd_root, state_thread, screen_capture, control_tx) {
                error!("WiFi file server failed: {:?}", err);
            }
        });

    if let Err(err) = spawn_result {
        error!("Failed to spawn WiFi file server thread: {:?}", err);
        if let Ok(mut guard) = state.lock() {
            guard.ssid = "WiFi disabled (low mem)".to_string();
            guard.ip = None;
        }
    }

    state
}

type ServerResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn bringup_wifi_and_server(
    modem: Modem,
    sd_root: Option<PathBuf>,
    state: WifiStateHandle,
    screen_capture: ScreenCaptureHandle,
    control_tx: Sender<RemoteCommand>,
) -> ServerResult<()> {
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?;

    let mut ssid = String::new();
    let mut password = String::new();
    let mut auto_connect = true;

    if let Some(ref root) = sd_root {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    let name_upper = name.to_uppercase();
                    // Match "wifi.conf", "WIFI.CONF", or "WIFI~1.CON" (8.3 alias)
                    if name_upper == "WIFI.CONF" || name_upper == "WIFI~1.CON" || name_upper == "WIFI.CON" {
                        let path = entry.path();
                        info!("Found WiFi config at: {:?}", path);
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(config) = serde_json::from_str::<WifiConfig>(&content) {
                                ssid = config.ssid;
                                password = config.password;
                                auto_connect = config.auto_connect;
                                break;
                            } else {
                                error!("Failed to parse JSON in {:?}", path);
                            }
                        }
                    }
                }
            }
        }

        if ssid.is_empty() {
            error!("WiFi config (wifi.conf) not found in {:?}", root);
            // List files to help debug if it still fails
            if let Ok(entries) = std::fs::read_dir(root) {
                info!("Files on SD:");
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        info!("  - {}", name);
                    }
                }
            }
        }
    }

    if ssid.is_empty() {
        error!("No WiFi credentials found on SD card (wifi.conf)");
        let mut guard = state.lock().unwrap();
        guard.ssid = "No config".to_string();
        return Ok(());
    }

    if !auto_connect {
        info!("WiFi autoConnect is false, skipping connection");
        let mut guard = state.lock().unwrap();
        guard.ssid = format!("{} (manual)", ssid);
        return Ok(());
    }

    {
        let mut guard = state.lock().unwrap();
        guard.ssid = ssid.clone();
    }

    let client_cfg = ClientConfiguration {
        ssid: ssid.as_str().try_into().unwrap(),
        password: password.as_str().try_into().unwrap(),
        ..Default::default()
    };

    wifi.set_configuration(&Configuration::Client(client_cfg))?;
    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;

    if let Ok(ip_info) = wifi.wifi().sta_netif().get_ip_info() {
        let mut guard = state.lock().unwrap();
        guard.ip = Some(ip_info.ip.to_string());
    }

    info!("WiFi connected to {}", ssid);

    let _server = launch_http(sd_root, state.clone(), screen_capture, control_tx)?;

    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

fn launch_http(
    sd_root: Option<PathBuf>,
    state: WifiStateHandle,
    screen_capture: ScreenCaptureHandle,
    control_tx: Sender<RemoteCommand>,
) -> ServerResult<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&HttpConfig {
        http_port: 8080,
        ..Default::default()
    })?;

    let index_state = state.clone();
    server.fn_handler("/", Method::Get, move |req| {
        let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        
        // Get dynamic values (small allocations)
        let (ssid, ip) = if let Ok(guard) = index_state.lock() {
            (
                guard.ssid.clone(),
                guard.ip.clone().unwrap_or_default(),
            )
        } else {
            ("Cardputer-RustOS".to_string(), String::new())
        };
        
        // Stream HTML from const parts (in Flash) - no heap allocation for the large parts
        resp.write_all(INDEX_HTML_PART1).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        resp.write_all(ssid.as_bytes()).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        resp.write_all(INDEX_HTML_PART2).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        resp.write_all(ip.as_bytes()).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        resp.write_all(INDEX_HTML_PART3).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    let screen_capture = screen_capture.clone();
    server.fn_handler("/api/screen", Method::Get, move |req| {
        let mut resp = req
            .into_response(
                200,
                Some("OK"),
                &[("Content-Type", "image/bmp"), ("Cache-Control", "no-store")],
            )
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

        let (width, height, pixels) = if let Ok(guard) = screen_capture.lock() {
            let (width, height) = guard.dimensions();
            (width, height, guard.pixels().to_vec())
        } else {
            (0, 0, Vec::new())
        };

        if width > 0 && height > 0 {
            let bmp = build_screen_bmp(width, height, &pixels);
            resp.write_all(&bmp).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }

        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    let control_tx = control_tx.clone();
    let launch_tx = control_tx.clone();
    server.fn_handler("/api/control", Method::Post, move |req| {
        let uri = req.uri().to_string();
        let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        if let Some(action) = parse_control_action(&uri) {
            let _ = control_tx.send(RemoteCommand::Menu(action));
            resp.write_all(b"OK").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        } else {
            resp.write_all(b"Invalid action").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    let launch_root = sd_root.clone();
    server.fn_handler("/api/launch", Method::Post, move |req| {
        let uri = req.uri().to_string();
        let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        if let Some(ref root) = launch_root {
            if let Some(pos) = uri.find("path=") {
                let p = &uri[pos + 5..];
                let subpath = p.replace("%2F", "/").replace("%2f", "/");
                let target = root.join(subpath.trim_start_matches('/'));
                if target.starts_with(root) && target.is_file() {
                    let ext = target
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.to_ascii_lowercase());
                    if let Some(ext) = ext.as_deref() {
                        let command = match ext {
                            "bin" => Some(RemoteCommand::FlashBin(target)),
                            "wasm" => Some(RemoteCommand::RunLive(LiveAppKind::Wasm, target)),
                            "py" | "mpy" => Some(RemoteCommand::RunLive(LiveAppKind::Python, target)),
                            _ => None,
                        };
                        if let Some(command) = command {
                            let _ = launch_tx.send(command);
                            resp.write_all(b"OK")
                                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                            return Ok::<(), Box<dyn std::error::Error>>(());
                        }
                    }
                }
            }
            resp.write_all(b"Invalid app").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        } else {
            resp.write_all(b"SD card not mounted").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    let list_root = sd_root.clone();
    server.fn_handler("/api/files", Method::Get, move |req| {
        let uri = req.uri().to_string();
        let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        
        if let Some(ref root) = list_root {
            // Very basic query param parsing for ?path=
            let subpath = if let Some(pos) = uri.find("path=") {
                let p = &uri[pos+5..];
                // Decode %2F to / (basic)
                p.replace("%2F", "/").replace("%2f", "/")
            } else {
                "/".to_string()
            };

            let target = if subpath == "/" || subpath.is_empty() {
                root.clone()
            } else {
                root.join(subpath.trim_start_matches('/'))
            };

            // Safety check: ensure target is within root
            if !target.starts_with(root) {
                 resp.write_all(b"[]").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                 return Ok(());
            }

            let mut entries = Vec::new();
            if let Ok(dir) = std::fs::read_dir(&target) {
                for entry in dir.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        entries.push(FileEntry {
                            name: entry.file_name().to_string_lossy().to_string(),
                            is_dir: meta.is_dir(),
                            size: meta.len(),
                        });
                    }
                }
            }
            let json = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string());
            resp.write_all(json.as_bytes()).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        } else {
            resp.write_all(b"[]").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }

        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    let delete_root = sd_root.clone();
    server.fn_handler("/api/delete", Method::Post, move |req| {
        let uri = req.uri().to_string();
        let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        if let Some(ref root) = delete_root {
            if let Some(pos) = uri.find("path=") {
                let p = &uri[pos+5..];
                let subpath = p.replace("%2F", "/").replace("%2f", "/");
                let target = root.join(subpath.trim_start_matches('/'));
                if target.starts_with(root) && target != *root {
                    if target.is_file() {
                        let _ = std::fs::remove_file(target);
                        resp.write_all(b"Deleted").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                    } else if target.is_dir() {
                        let _ = std::fs::remove_dir_all(target);
                        resp.write_all(b"Deleted Directory").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                    }
                }
            }
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    let download_root = sd_root.clone();
    server.fn_handler("/api/download", Method::Get, move |req| {
        let uri = req.uri().to_string();
        if let Some(ref root) = download_root {
            if let Some(pos) = uri.find("path=") {
                let p = &uri[pos+5..];
                let subpath = p.replace("%2F", "/").replace("%2f", "/");
                let target = root.join(subpath.trim_start_matches('/'));
                if target.starts_with(root) && target.is_file() {
                    let mut file = File::open(&target).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                    let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                    let mut buf = [0u8; 4096];
                    loop {
                        let n = file.read(&mut buf).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                        if n == 0 { break; }
                        resp.write_all(&buf[..n]).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                    }
                    return Ok::<(), Box<dyn std::error::Error>>(());
                }
            }
        }
        let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        resp.write_all(b"Not found").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    let mkdir_root = sd_root.clone();
    server.fn_handler("/api/mkdir", Method::Post, move |req| {
        let uri = req.uri().to_string();
        let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        if let Some(ref root) = mkdir_root {
            if let Some(pos) = uri.find("path=") {
                let p = &uri[pos+5..];
                let subpath = p.replace("%2F", "/").replace("%2f", "/");
                let target = root.join(subpath.trim_start_matches('/'));
                if target.starts_with(root) {
                    let _ = std::fs::create_dir_all(target);
                    resp.write_all(b"Created").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                }
            }
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    server.fn_handler("/api/reboot_factory", Method::Post, move |req| -> Result<(), Box<dyn std::error::Error>> {
        let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        resp.write_all(b"Rebooting to Factory OS...").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        chainload::reboot_to_factory()
    })?;


    let upload_root = sd_root.clone();
    server.fn_handler("/upload", Method::Post, move |mut req| {
        // ... (existing upload logic, updated for path support)
        if upload_root.is_none() {
            let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            resp.write_all(b"SD card not mounted").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            return Ok(());
        }

        let filename = req.header("X-Filename").map(str::to_owned).unwrap_or_else(|| "upload.bin".to_string());
        let path = req.header("X-Path").map(str::to_owned).unwrap_or_else(|| "/".to_string());

        let target = upload_root.as_ref().unwrap()
            .join(path.trim_start_matches('/'))
            .join(&filename);

        if !target.starts_with(upload_root.as_ref().unwrap()) {
             let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
             resp.write_all(b"Invalid target").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
             return Ok(());
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }

        let mut file = File::create(&target).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        // Avoid blowing the httpd task stack; use a heap buffer instead.
        let mut buf = vec![0u8; 1024];
        loop {
            let read = req.read(&mut buf).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            if read == 0 { break; }
            file.write_all(&buf[..read]).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }

        let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        resp.write_all(b"OK").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    Ok(server)
}

// HTML template split into const parts (stored in Flash, not heap)
// PART1: From start to just before {ssid}
const INDEX_HTML_PART1: &[u8] = br#"<!doctype html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Cardputer RustOS | Web UI</title>
    <style>
        :root {
            --bg-color: #f6f1e7;
            --glass-bg: rgba(255, 255, 255, 0.78);
            --glass-border: rgba(39, 46, 58, 0.12);
            --accent-primary: #2a7f7a;
            --accent-secondary: #e07a5f;
            --text-main: #2a2a2a;
            --text-dim: #61656c;
            --danger: #c2413d;
        }

        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: 'Space Grotesk', 'Avenir Next', 'Trebuchet MS', sans-serif;
            background: var(--bg-color);
            background-image:
                radial-gradient(circle at 12% 12%, rgba(224, 122, 95, 0.22), transparent 45%),
                radial-gradient(circle at 88% 18%, rgba(42, 127, 122, 0.18), transparent 42%),
                radial-gradient(circle at 40% 92%, rgba(240, 194, 123, 0.22), transparent 50%),
                linear-gradient(120deg, #f6f1e7 0%, #fbf7ef 100%);
            color: var(--text-main);
            min-height: 100vh;
            display: flex;
            justify-content: center;
            padding: 2rem;
        }

        .container {
            width: 100%;
            max-width: 900px;
            background: var(--glass-bg);
            backdrop-filter: blur(12px);
            -webkit-backdrop-filter: blur(12px);
            border: 1px solid var(--glass-border);
            border-radius: 24px;
            padding: 2.5rem;
            box-shadow: 0 24px 40px rgba(30, 36, 45, 0.18);
            animation: fadeIn 0.6s ease both;
        }

        @keyframes fadeIn {
            from { opacity: 0; transform: translateY(6px); }
            to { opacity: 1; transform: translateY(0); }
        }

        @keyframes riseIn {
            from { opacity: 0; transform: translateY(14px); }
            to { opacity: 1; transform: translateY(0); }
        }

        header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 2rem;
        }

        .brand { display: flex; align-items: center; gap: 12px; }
        .brand h1 { font-family: 'Fraunces', 'Palatino Linotype', serif; font-size: 1.6rem; font-weight: 700; letter-spacing: 0.02em; background: linear-gradient(to right, var(--accent-primary), var(--accent-secondary)); -webkit-background-clip: text; -webkit-text-fill-color: transparent; }
        
        .info { text-align: right; font-size: 0.875rem; color: var(--text-dim); }
        .info span { display: block; }

        .dashboard {
            display: grid;
            grid-template-columns: 1.2fr 0.8fr;
            gap: 20px;
            margin-bottom: 2rem;
        }

        .panel {
            background: rgba(255, 255, 255, 0.62);
            border-radius: 20px;
            border: 1px solid var(--glass-border);
            padding: 1.5rem;
            box-shadow: 0 14px 24px rgba(36, 42, 52, 0.08);
            animation: riseIn 0.7s ease both;
        }

        .dashboard .panel:nth-child(2) { animation-delay: 0.08s; }

        .panel-header {
            display: flex;
            justify-content: space-between;
            align-items: baseline;
            margin-bottom: 1rem;
        }

        .panel-header h2 { font-size: 0.9rem; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; }
        .panel-sub { font-size: 0.75rem; color: var(--text-dim); }

        .screen-frame {
            position: relative;
            background: rgba(255, 255, 255, 0.6);
            border-radius: 16px;
            padding: 12px;
            border: 1px solid var(--glass-border);
            display: flex;
            justify-content: center;
            align-items: center;
        }

        #screenImg {
            width: 100%;
            max-width: 420px;
            aspect-ratio: 240 / 135;
            image-rendering: pixelated;
            border-radius: 10px;
            border: 1px solid rgba(0,0,0,0.08);
            background: #111418;
        }

        .screen-badge {
            position: absolute;
            top: 10px;
            left: 12px;
            font-size: 0.65rem;
            letter-spacing: 0.08em;
            padding: 4px 8px;
            border-radius: 999px;
            background: rgba(42, 127, 122, 0.2);
            border: 1px solid rgba(42, 127, 122, 0.45);
        }

        .screen-badge.paused {
            background: rgba(97, 101, 108, 0.16);
            border-color: rgba(97, 101, 108, 0.32);
        }

        .screen-meta {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-top: 0.75rem;
            font-size: 0.8rem;
            color: var(--text-dim);
        }

        .btn-soft {
            background: rgba(255, 255, 255, 0.8);
            border: 1px solid var(--glass-border);
            color: var(--text-main);
            padding: 6px 12px;
            border-radius: 10px;
            font-size: 0.75rem;
            cursor: pointer;
            transition: 0.2s;
        }

        .btn-soft:hover {
            background: rgba(255, 255, 255, 0.96);
        }

        .control-pad {
            display: grid;
            grid-template-columns: repeat(3, 1fr);
            gap: 10px;
            align-items: center;
            justify-items: center;
        }

        .ctrl-btn {
            width: 100%;
            padding: 10px 12px;
            border-radius: 12px;
            border: 1px solid var(--glass-border);
            background: rgba(255, 255, 255, 0.78);
            color: var(--text-main);
            font-weight: 600;
            cursor: pointer;
            transition: 0.2s;
        }

        .ctrl-btn:hover {
            background: rgba(255, 255, 255, 0.96);
            transform: translateY(-1px);
        }

        .ctrl-btn.primary {
            background: linear-gradient(to right, var(--accent-primary), var(--accent-secondary));
            border: none;
        }

        .control-hint {
            margin-top: 0.75rem;
            font-size: 0.75rem;
            color: var(--text-dim);
            text-align: center;
        }

        .control-spacer {
            height: 100%;
            width: 100%;
        }

        .breadcrumb {
            display: flex;
            gap: 8px;
            margin-bottom: 1.5rem;
            font-size: 0.875rem;
            color: var(--text-dim);
        }
        .breadcrumb span { cursor: pointer; color: var(--text-main); }
        .breadcrumb span:hover { text-decoration: underline; }

        .file-list {
            background: rgba(255, 255, 255, 0.7);
            border-radius: 16px;
            overflow: hidden;
            border: 1px solid var(--glass-border);
            box-shadow: 0 12px 22px rgba(33, 40, 52, 0.06);
            animation: riseIn 0.7s ease both;
            animation-delay: 0.12s;
        }

        .file-item {
            display: grid;
            grid-template-columns: auto 1fr auto auto;
            align-items: center;
            padding: 12px 20px;
            gap: 16px;
            border-bottom: 1px solid var(--glass-border);
            transition: background 0.2s;
        }
        .file-item:last-child { border-bottom: none; }
        .file-item:hover { background: rgba(42, 127, 122, 0.06); }

        .icon { width: 20px; height: 20px; color: var(--accent-primary); }
        .name { font-size: 0.9375rem; font-weight: 500; cursor: pointer; }
        .size { font-size: 0.8125rem; color: var(--text-dim); }
        
        .actions { display: flex; gap: 8px; }
        .btn-action { color: var(--accent-primary); background: none; border: none; cursor: pointer; transition: 0.2s; padding: 4px; }
        .btn-action:hover { transform: scale(1.1); filter: brightness(1.2); }
        .btn-del { color: var(--danger); background: none; border: none; cursor: pointer; opacity: 0.6; transition: 0.2s; padding: 4px; }
        .btn-del:hover { opacity: 1; transform: scale(1.1); }

        .btn-main { 
            background: linear-gradient(to right, var(--accent-primary), var(--accent-secondary));
            border: none; color: white; padding: 10px 20px; border-radius: 12px; font-weight: 600; cursor: pointer; margin-bottom: 1rem;
            transition: 0.3s; box-shadow: 0 6px 16px rgba(42, 127, 122, 0.25);
        }
        .btn-main:hover { transform: translateY(-2px); box-shadow: 0 8px 20px rgba(42, 127, 122, 0.35); }

        .upload-section {
            margin-top: 2.5rem;
            border: 2px dashed var(--glass-border);
            border-radius: 20px;
            padding: 2rem;
            text-align: center;
            transition: 0.3s;
            animation: riseIn 0.7s ease both;
            animation-delay: 0.16s;
        }
        .upload-section.dragover { border-color: var(--accent-primary); background: rgba(42, 127, 122, 0.08); }
        
        .upload-controls { display: flex; justify-content: center; gap: 20px; align-items: center; margin-bottom: 20px; }

        .upload-label { display: block; cursor: pointer; }
        .upload-label span { display: block; margin-bottom: 8px; font-weight: 600; color: var(--accent-primary); }
        .upload-label small { color: var(--text-dim); }

        #fileInput { display: none; }

        .progress-container {
            margin-top: 1.5rem;
            height: 8px;
            background: rgba(0, 0, 0, 0.06);
            border-radius: 4px;
            overflow: hidden;
            display: none;
        }
        .progress-bar { height: 100%; width: 0%; background: linear-gradient(to right, var(--accent-primary), var(--accent-secondary)); transition: width 0.3s; }

        @media (prefers-reduced-motion: reduce) {
            * { animation: none !important; transition: none !important; }
        }

        @media (max-width: 640px) {
            body { padding: 1rem; }
            .container { padding: 1.5rem; }
            .dashboard { grid-template-columns: 1fr; }
            .panel { padding: 1.25rem; }
        }
    </style>
</head>
<body>
    <div class="container">
        <header>
            <div class="brand">
                <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>
                <h1>Cardputer OS</h1>
            </div>
            <div class="info">
                <span>SSID: <b>"#;

// PART2: Between {ssid} and {ip}
const INDEX_HTML_PART2: &[u8] = br#"</b></span>
                <span>IP: <b>"#;

// PART3: After {ip} to end
const INDEX_HTML_PART3: &[u8] = br#"</b></span>
                <button class="btn-action" style="margin-top: 8px; font-size: 11px; color: var(--text-dim);" onclick="rebootFactory()">Reset Boot to Factory</button>
            </div>
        </header>

        <div class="dashboard">
            <div class="panel screen-panel">
                <div class="panel-header">
                    <h2>Live Screen</h2>
                    <span class="panel-sub">Framebuffer mirror</span>
                </div>
                <div class="screen-frame">
                    <img id="screenImg" alt="Cardputer screen" src="" width="240" height="135">
                    <div class="screen-badge" id="screenBadge">LIVE</div>
                </div>
                <div class="screen-meta">
                    <span>240x135</span>
                    <button class="btn-soft" onclick="toggleScreen()" id="screenToggle">Pause</button>
                </div>
            </div>
            <div class="panel controls-panel">
                <div class="panel-header">
                    <h2>Remote Control</h2>
                    <span class="panel-sub">Menu navigation</span>
                </div>
                <div class="control-pad">
                    <div class="control-spacer"></div>
                    <button class="ctrl-btn" onclick="sendControl('up')">Up</button>
                    <div class="control-spacer"></div>
                    <button class="ctrl-btn" onclick="sendControl('back')">Back</button>
                    <button class="ctrl-btn primary" onclick="sendControl('select')">Select</button>
                    <button class="ctrl-btn" onclick="sendControl('refresh')">Refresh</button>
                    <div class="control-spacer"></div>
                    <button class="ctrl-btn" onclick="sendControl('down')">Down</button>
                    <div class="control-spacer"></div>
                </div>
                <div class="control-hint">Arrow keys, Enter, Backspace, and R work here.</div>
            </div>
        </div>

        <div id="breadcrumb" class="breadcrumb"></div>

        <div id="fileList" class="file-list">
            <!-- Files loaded here -->
        </div>

        <div class="upload-section" id="dropZone">
            <div class="upload-controls">
                <button class="btn-main" onclick="mkdir()">+ New Folder</button>
                <label for="fileInput" class="btn-main" style="margin-bottom: 1rem;">&uarr; Upload Files</label>
            </div>
            <input type="file" id="fileInput" multiple>
            <label for="fileInput" class="upload-label">
                <span>Or drop files here</span>
                <small>Max upload size: SD Card limit</small>
            </label>
            <div id="progressContainer" class="progress-container">
                <div id="progressBar" class="progress-bar"></div>
            </div>
        </div>
    </div>

    <script>
        let currentPath = '/';
        const fileList = document.getElementById('fileList');
        const breadcrumb = document.getElementById('breadcrumb');
        const dropZone = document.getElementById('dropZone');
        const fileInput = document.getElementById('fileInput');
        const progressBar = document.getElementById('progressBar');
        const progressContainer = document.getElementById('progressContainer');
        const screenImg = document.getElementById('screenImg');
        const screenBadge = document.getElementById('screenBadge');
        const screenToggle = document.getElementById('screenToggle');
        let screenActive = true;
        let screenTimer = null;
        let lastControlTime = 0;
        const CONTROL_THROTTLE_MS = 80;

        function scheduleScreen() {
            if (!screenActive) return;
            clearTimeout(screenTimer);
            screenTimer = setTimeout(() => {
                screenImg.src = `/api/screen?ts=${Date.now()}`;
            }, 200);
        }

        function toggleScreen() {
            screenActive = !screenActive;
            screenBadge.textContent = screenActive ? 'LIVE' : 'PAUSED';
            screenBadge.classList.toggle('paused', !screenActive);
            screenToggle.textContent = screenActive ? 'Pause' : 'Resume';
            if (screenActive) {
                scheduleScreen();
            } else {
                clearTimeout(screenTimer);
            }
        }

        function sendControl(action) {
            const now = Date.now();
            if (now - lastControlTime < CONTROL_THROTTLE_MS) return;
            lastControlTime = now;
            fetch(`/api/control?action=${encodeURIComponent(action)}`, { method: 'POST' }).catch(() => {});
        }

        screenImg.onload = scheduleScreen;
        screenImg.onerror = scheduleScreen;
        scheduleScreen();

        document.addEventListener('keydown', (e) => {
            if (document.activeElement && document.activeElement.tagName === 'INPUT') return;
            let action = null;
            if (e.key === 'ArrowUp') action = 'up';
            if (e.key === 'ArrowDown') action = 'down';
            if (e.key === 'Enter') action = 'select';
            if (e.key === 'Backspace' || e.key === 'Escape') action = 'back';
            if (e.key === 'r' || e.key === 'R') action = 'refresh';
            if (action) {
                e.preventDefault();
                sendControl(action);
            }
        });

        async function loadFiles(path = '/') {
            currentPath = path;
            renderBreadcrumbs();
            fileList.innerHTML = '<div style="padding: 20px; text-align: center;">Loading...</div>';
            
            try {
                const resp = await fetch(`/api/files?path=${encodeURIComponent(path)}`);
                const files = await resp.json();
                
                fileList.innerHTML = '';
                
                if (path !== '/') {
                    const parent = path.split('/').slice(0, -1).join('/') || '/';
                    addFileItem({ name: '..', is_dir: true, size: 0 }, parent);
                }

                files.sort((a,b) => (b.is_dir - a.is_dir) || a.name.localeCompare(b.name))
                     .forEach(f => addFileItem(f));
                
                if (files.length === 0 && path === '/') {
                    fileList.innerHTML = '<div style="padding: 20px; text-align: center; color: var(--text-dim);">No files found</div>';
                }
            } catch (err) {
                fileList.innerHTML = '<div style="padding: 20px; text-align: center; color: var(--danger);">Error loading files</div>';
            }
        }

        function addFileItem(file, overridePath = null) {
            const div = document.createElement('div');
            div.className = 'file-item';
            const lowerName = file.name.toLowerCase();
            const isBin = !file.is_dir && lowerName.endsWith('.bin');
            const isWasm = !file.is_dir && lowerName.endsWith('.wasm');
            const isPy = !file.is_dir && (lowerName.endsWith('.py') || lowerName.endsWith('.mpy'));
            const isLiveApp = isWasm || isPy;
            
            const icon = file.is_dir 
                ? '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color: #fbbf24"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>'
                : '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"></path><polyline points="13 2 13 9 20 9"></polyline></svg>';

            div.innerHTML = `
                ${icon}
                <div class="name">${file.name}</div>
                <div class="size">${file.is_dir ? '-' : formatBytes(file.size)}</div>
                <div class="actions">
                    ${isLiveApp ? `<button class="btn-action" title="Run" onclick="launchApp('${file.name}')"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"></path><polygon points="10 8 16 12 10 16 10 8"></polygon></svg></button>` : ''}
                    ${isBin ? `<button class="btn-action" title="Flash & Reboot" onclick="launchApp('${file.name}')"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M13 2L3 14h7l-1 8 12-14h-7l1-6z"></path></svg></button>` : ''}
                    ${!file.is_dir ? `<button class="btn-action" title="Download" onclick="downloadFile('${file.name}')"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3"></path></svg></button>` : ''}
                    ${file.name !== '..' ? `<button class="btn-del" title="Delete" onclick="deleteFile('${file.name}')"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></button>` : ''}
                </div>
            `;

            div.querySelector('.name').onclick = () => {
                if (file.is_dir) {
                    const newPath = overridePath || (currentPath === '/' ? '/' + file.name : currentPath + '/' + file.name);
                    loadFiles(newPath);
                }
            };

            fileList.appendChild(div);
        }

        function renderBreadcrumbs() {
            const parts = currentPath.split('/').filter(p => p);
            breadcrumb.innerHTML = '<span onclick="loadFiles(\'/\')">Root</span>';
            let path = '';
            parts.forEach(p => {
                path += '/' + p;
                const linkPath = path;
                breadcrumb.innerHTML += ` / <span onclick="loadFiles('${linkPath}')">${p}</span>`;
            });
        }

        function downloadFile(name) {
            const path = currentPath === '/' ? '/' + name : currentPath + '/' + name;
            window.location.href = `/api/download?path=${encodeURIComponent(path)}`;
        }

        async function launchApp(name) {
            const lowerName = name.toLowerCase();
            const isBin = lowerName.endsWith('.bin');
            const prompt = isBin
                ? `Flash and reboot into ${name}?`
                : `Run ${name} now?`;
            if (!confirm(prompt)) return;
            const path = currentPath === '/' ? '/' + name : currentPath + '/' + name;
            const resp = await fetch(`/api/launch?path=${encodeURIComponent(path)}`, { method: 'POST' });
            const text = await resp.text();
            if (text.trim() !== 'OK') {
                alert(text || 'Launch failed');
            }
        }

        async function mkdir() {
            const name = prompt('Folder name:');
            if (!name) return;
            const path = currentPath === '/' ? '/' + name : currentPath + '/' + name;
            await fetch(`/api/mkdir?path=${encodeURIComponent(path)}`, { method: 'POST' });
            loadFiles(currentPath);
        }

        async function deleteFile(name) {
            if (!confirm(`Delete ${name}?`)) return;
            const path = currentPath === '/' ? '/' + name : currentPath + '/' + name;
            await fetch(`/api/delete?path=${encodeURIComponent(path)}`, { method: 'POST' });
            loadFiles(currentPath);
        }

        function formatBytes(bytes) {
            if (bytes === 0) return '0 B';
            const k = 1024;
            const sizes = ['B', 'KB', 'MB', 'GB'];
            const i = Math.floor(Math.log(bytes) / Math.log(k));
            return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
        }

        // Upload Logic
        fileInput.onchange = (e) => uploadFiles(e.target.files);
        
        dropZone.ondragover = (e) => { e.preventDefault(); dropZone.classList.add('dragover'); };
        dropZone.ondragleave = () => dropZone.classList.remove('dragover');
        dropZone.ondrop = (e) => { 
            e.preventDefault(); 
            dropZone.classList.remove('dragover');
            uploadFiles(e.dataTransfer.files);
        };

        async function uploadFiles(files) {
            if (!files.length) return;
            progressContainer.style.display = 'block';
            
            for (let file of files) {
                await new Promise((resolve, reject) => {
                    const xhr = new XMLHttpRequest();
                    xhr.open('POST', '/upload');
                    xhr.setRequestHeader('X-Filename', file.name);
                    xhr.setRequestHeader('X-Path', currentPath);
                    
                    xhr.upload.onprogress = (e) => {
                        if (e.lengthComputable) {
                            const percent = (e.loaded / e.total) * 100;
                            progressBar.style.width = percent + '%';
                        }
                    };
                    
                    xhr.onload = () => resolve();
                    xhr.onerror = () => reject();
                    xhr.send(file);
                });
            }
            
            progressBar.style.width = '0%';
            progressContainer.style.display = 'none';
            loadFiles(currentPath);
        }

        async function rebootFactory() {
            if (!confirm('Reboot back to the main Factory OS?')) return;
            await fetch('/api/reboot_factory', { method: 'POST' });
            setTimeout(() => location.reload(), 2000);
        }

        loadFiles();
    </script>
</body>
</html>
"#;

fn parse_control_action(uri: &str) -> Option<MenuAction> {
    let query = uri.splitn(2, '?').nth(1)?;
    for part in query.split('&') {
        let mut iter = part.splitn(2, '=');
        let key = iter.next()?;
        let value = iter.next().unwrap_or("");
        if key == "action" {
            return match value {
                "up" => Some(MenuAction::Up),
                "down" => Some(MenuAction::Down),
                "select" => Some(MenuAction::Select),
                "back" => Some(MenuAction::Back),
                "refresh" => Some(MenuAction::Refresh),
                _ => None,
            };
        }
    }
    None
}

fn build_screen_bmp(width: usize, height: usize, pixels: &[u16]) -> Vec<u8> {
    let row_stride = ((width * 3 + 3) / 4) * 4;
    let image_size = row_stride * height;
    let file_size = 54 + image_size;

    let mut data = Vec::with_capacity(file_size);
    data.extend_from_slice(b"BM");
    data.extend_from_slice(&(file_size as u32).to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&54u32.to_le_bytes());

    data.extend_from_slice(&40u32.to_le_bytes());
    data.extend_from_slice(&(width as i32).to_le_bytes());
    data.extend_from_slice(&(height as i32).to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&24u16.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(image_size as u32).to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    let padding = row_stride - width * 3;
    for y in (0..height).rev() {
        let row_start = y * width;
        for x in 0..width {
            let pixel = pixels.get(row_start + x).copied().unwrap_or(0);
            let r = ((pixel >> 11) & 0x1f) as u8;
            let g = ((pixel >> 5) & 0x3f) as u8;
            let b = (pixel & 0x1f) as u8;
            let r8 = (r << 3) | (r >> 2);
            let g8 = (g << 2) | (g >> 4);
            let b8 = (b << 3) | (b >> 2);
            data.push(b8);
            data.push(g8);
            data.push(r8);
        }
        for _ in 0..padding {
            data.push(0);
        }
    }

    data
}
