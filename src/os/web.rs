use std::fs::File;
use std::io::{Read, Write as StdWrite};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write as HttpWrite;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys;
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

pub fn start_wifi_file_server(modem: Modem, sd_root: Option<PathBuf>) -> WifiStateHandle {
    let state = Arc::new(Mutex::new(WifiState {
        mode: WifiMode::Station,
        ssid: "Checking SD...".to_string(),
        ip: None,
    }));

    let state_thread = state.clone();
    thread::Builder::new()
        .stack_size(32768)
        .spawn(move || {
            if let Err(err) = bringup_wifi_and_server(modem, sd_root, state_thread) {
                error!("WiFi file server failed: {:?}", err);
            }
        })
        .unwrap();

    state
}

type ServerResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn bringup_wifi_and_server(
    modem: Modem,
    sd_root: Option<PathBuf>,
    state: WifiStateHandle,
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

    let _server = launch_http(sd_root, state.clone())?;

    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

fn launch_http(sd_root: Option<PathBuf>, state: WifiStateHandle) -> ServerResult<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&HttpConfig {
        http_port: 8080,
        ..Default::default()
    })?;

    let index_body = render_index(&state);
    server.fn_handler("/", Method::Get, move |req| {
        let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        resp.write_all(index_body.as_bytes()).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
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

    server.fn_handler("/api/reboot_factory", Method::Post, move |req| {
        let mut resp = req.into_ok_response().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        resp.write_all(b"Rebooting to Factory OS...").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        unsafe {
            let factory = sys::esp_partition_find_first(
                sys::esp_partition_type_t_ESP_PARTITION_TYPE_APP,
                sys::esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_APP_FACTORY,
                core::ptr::null(),
            );
            if !factory.is_null() {
                sys::esp_ota_set_boot_partition(factory);
            }
            sys::esp_restart();
        }
        Ok::<(), Box<dyn std::error::Error>>(())
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
        let mut buf = [0u8; 4096]; // Larger buffer for faster upload
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

fn render_index(state: &WifiStateHandle) -> String {
    let guard = state.lock().ok();
    let (ssid, ip) = guard
        .as_deref()
        .map(|s| (s.ssid.clone(), s.ip.clone().unwrap_or_else(|| "".to_string())))
        .unwrap_or_else(|| ("Cardputer-RustOS".to_string(), String::new()));

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Cardputer RustOS | File Manager</title>
    <style>
        :root {{
            --bg-color: #0c0d12;
            --glass-bg: rgba(255, 255, 255, 0.05);
            --glass-border: rgba(255, 255, 255, 0.1);
            --accent-primary: #8b5cf6;
            --accent-secondary: #d946ef;
            --text-main: #f8fafc;
            --text-dim: #94a3b8;
            --danger: #ef4444;
        }}

        * {{ box-sizing: border-box; margin: 0; padding: 0; }}
        body {{
            font-family: 'Inter', -apple-system, sans-serif;
            background: var(--bg-color);
            background-image: 
                radial-gradient(at 0% 0%, hsla(253,16%,7%,1) 0, transparent 50%), 
                radial-gradient(at 50% 0%, hsla(225,39%,30%,1) 0, transparent 50%), 
                radial-gradient(at 100% 0%, hsla(339,49%,30%,1) 0, transparent 50%);
            color: var(--text-main);
            min-height: 100vh;
            display: flex;
            justify-content: center;
            padding: 2rem;
        }}

        .container {{
            width: 100%;
            max-width: 900px;
            background: var(--glass-bg);
            backdrop-filter: blur(12px);
            -webkit-backdrop-filter: blur(12px);
            border: 1px solid var(--glass-border);
            border-radius: 24px;
            padding: 2.5rem;
            box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
        }}

        header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 2rem;
        }}

        .brand {{ display: flex; align-items: center; gap: 12px; }}
        .brand h1 {{ font-size: 1.5rem; font-weight: 700; background: linear-gradient(to right, var(--accent-primary), var(--accent-secondary)); -webkit-background-clip: text; -webkit-text-fill-color: transparent; }}
        
        .info {{ text-align: right; font-size: 0.875rem; color: var(--text-dim); }}
        .info span {{ display: block; }}

        .breadcrumb {{
            display: flex;
            gap: 8px;
            margin-bottom: 1.5rem;
            font-size: 0.875rem;
            color: var(--text-dim);
        }}
        .breadcrumb span {{ cursor: pointer; color: var(--text-main); }}
        .breadcrumb span:hover {{ text-decoration: underline; }}

        .file-list {{
            background: rgba(0,0,0,0.2);
            border-radius: 16px;
            overflow: hidden;
            border: 1px solid var(--glass-border);
        }}

        .file-item {{
            display: grid;
            grid-template-columns: auto 1fr auto auto;
            align-items: center;
            padding: 12px 20px;
            gap: 16px;
            border-bottom: 1px solid var(--glass-border);
            transition: background 0.2s;
        }}
        .file-item:last-child {{ border-bottom: none; }}
        .file-item:hover {{ background: rgba(255,255,255,0.03); }}

        .icon {{ width: 20px; height: 20px; color: var(--accent-primary); }}
        .name {{ font-size: 0.9375rem; font-weight: 500; cursor: pointer; }}
        .size {{ font-size: 0.8125rem; color: var(--text-dim); }}
        
        .actions {{ display: flex; gap: 8px; }}
        .btn-action {{ color: var(--accent-primary); background: none; border: none; cursor: pointer; transition: 0.2s; padding: 4px; }}
        .btn-action:hover {{ transform: scale(1.1); filter: brightness(1.2); }}
        .btn-del {{ color: var(--danger); background: none; border: none; cursor: pointer; opacity: 0.6; transition: 0.2s; padding: 4px; }}
        .btn-del:hover {{ opacity: 1; transform: scale(1.1); }}

        .btn-main {{ 
            background: linear-gradient(to right, var(--accent-primary), var(--accent-secondary));
            border: none; color: white; padding: 10px 20px; border-radius: 12px; font-weight: 600; cursor: pointer; margin-bottom: 1rem;
            transition: 0.3s; box-shadow: 0 4px 15px rgba(139, 92, 246, 0.3);
        }}
        .btn-main:hover {{ transform: translateY(-2px); box-shadow: 0 6px 20px rgba(139, 92, 246, 0.5); }}

        .upload-section {{
            margin-top: 2.5rem;
            border: 2px dashed var(--glass-border);
            border-radius: 20px;
            padding: 2rem;
            text-align: center;
            transition: 0.3s;
        }}
        .upload-section.dragover {{ border-color: var(--accent-primary); background: rgba(139, 92, 246, 0.05); }}
        
        .upload-controls {{ display: flex; justify-content: center; gap: 20px; align-items: center; margin-bottom: 20px; }}

        .upload-label {{ display: block; cursor: pointer; }}
        .upload-label span {{ display: block; margin-bottom: 8px; font-weight: 600; color: var(--accent-primary); }}
        .upload-label small {{ color: var(--text-dim); }}

        #fileInput {{ display: none; }}

        .progress-container {{
            margin-top: 1.5rem;
            height: 8px;
            background: rgba(255,255,255,0.05);
            border-radius: 4px;
            overflow: hidden;
            display: none;
        }}
        .progress-bar {{ height: 100%; width: 0%; background: linear-gradient(to right, var(--accent-primary), var(--accent-secondary)); transition: width 0.3s; }}

        @media (max-width: 640px) {{
            body {{ padding: 1rem; }}
            .container {{ padding: 1.5rem; }}
        }}
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
                <span>SSID: <b>{ssid}</b></span>
                <span>IP: <b>{ip}</b></span>
                <button class="btn-action" style="margin-top: 8px; font-size: 11px; color: var(--text-dim);" onclick="rebootFactory()">Reset Boot to Factory</button>
            </div>
        </header>

        <div id="breadcrumb" class="breadcrumb"></div>

        <div id="fileList" class="file-list">
            <!-- Files loaded here -->
        </div>

        <div class="upload-section" id="dropZone">
            <div class="upload-controls">
                <button class="btn-main" onclick="mkdir()">+ New Folder</button>
                <label for="fileInput" class="btn-main" style="margin-bottom: 1rem;">↑ Upload Files</label>
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

        async function loadFiles(path = '/') {{
            currentPath = path;
            renderBreadcrumbs();
            fileList.innerHTML = '<div style="padding: 20px; text-align: center;">Loading...</div>';
            
            try {{
                const resp = await fetch(`/api/files?path=${{encodeURIComponent(path)}}`);
                const files = await resp.json();
                
                fileList.innerHTML = '';
                
                if (path !== '/') {{
                    const parent = path.split('/').slice(0, -1).join('/') || '/';
                    addFileItem({{ name: '..', is_dir: true, size: 0 }}, parent);
                }}

                files.sort((a,b) => (b.is_dir - a.is_dir) || a.name.localeCompare(b.name))
                     .forEach(f => addFileItem(f));
                
                if (files.length === 0 && path === '/') {{
                    fileList.innerHTML = '<div style="padding: 20px; text-align: center; color: var(--text-dim);">No files found</div>';
                }}
            }} catch (err) {{
                fileList.innerHTML = '<div style="padding: 20px; text-align: center; color: var(--danger);">Error loading files</div>';
            }}
        }}

        function addFileItem(file, overridePath = null) {{
            const div = document.createElement('div');
            div.className = 'file-item';
            
            const icon = file.is_dir 
                ? '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color: #fbbf24"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>'
                : '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"></path><polyline points="13 2 13 9 20 9"></polyline></svg>';

            div.innerHTML = `
                ${{icon}}
                <div class="name">${{file.name}}</div>
                <div class="size">${{file.is_dir ? '-' : formatBytes(file.size)}}</div>
                <div class="actions">
                    ${{!file.is_dir ? `<button class="btn-action" title="Download" onclick="downloadFile('${{file.name}}')"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3"></path></svg></button>` : ''}}
                    ${{file.name !== '..' ? `<button class="btn-del" title="Delete" onclick="deleteFile('${{file.name}}')"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></button>` : ''}}
                </div>
            `;

            div.querySelector('.name').onclick = () => {{
                if (file.is_dir) {{
                    const newPath = overridePath || (currentPath === '/' ? '/' + file.name : currentPath + '/' + file.name);
                    loadFiles(newPath);
                }}
            }};

            fileList.appendChild(div);
        }}

        function renderBreadcrumbs() {{
            const parts = currentPath.split('/').filter(p => p);
            breadcrumb.innerHTML = '<span onclick="loadFiles(\'/\')">Root</span>';
            let path = '';
            parts.forEach(p => {{
                path += '/' + p;
                const linkPath = path;
                breadcrumb.innerHTML += ` / <span onclick="loadFiles('${{linkPath}}')">${{p}}</span>`;
            }});
        }}

        function downloadFile(name) {{
            const path = currentPath === '/' ? '/' + name : currentPath + '/' + name;
            window.location.href = `/api/download?path=${{encodeURIComponent(path)}}`;
        }}

        async function mkdir() {{
            const name = prompt('Folder name:');
            if (!name) return;
            const path = currentPath === '/' ? '/' + name : currentPath + '/' + name;
            await fetch(`/api/mkdir?path=${{encodeURIComponent(path)}}`, {{ method: 'POST' }});
            loadFiles(currentPath);
        }}

        async function deleteFile(name) {{
            if (!confirm(`Delete ${{name}}?`)) return;
            const path = currentPath === '/' ? '/' + name : currentPath + '/' + name;
            await fetch(`/api/delete?path=${{encodeURIComponent(path)}}`, {{ method: 'POST' }});
            loadFiles(currentPath);
        }}

        function formatBytes(bytes) {{
            if (bytes === 0) return '0 B';
            const k = 1024;
            const sizes = ['B', 'KB', 'MB', 'GB'];
            const i = Math.floor(Math.log(bytes) / Math.log(k));
            return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
        }}

        // Upload Logic
        fileInput.onchange = (e) => uploadFiles(e.target.files);
        
        dropZone.ondragover = (e) => {{ e.preventDefault(); dropZone.classList.add('dragover'); }};
        dropZone.ondragleave = () => dropZone.classList.remove('dragover');
        dropZone.ondrop = (e) => {{ 
            e.preventDefault(); 
            dropZone.classList.remove('dragover');
            uploadFiles(e.dataTransfer.files);
        }};

        async function uploadFiles(files) {{
            if (!files.length) return;
            progressContainer.style.display = 'block';
            
            for (let file of files) {{
                await new Promise((resolve, reject) => {{
                    const xhr = new XMLHttpRequest();
                    xhr.open('POST', '/upload');
                    xhr.setRequestHeader('X-Filename', file.name);
                    xhr.setRequestHeader('X-Path', currentPath);
                    
                    xhr.upload.onprogress = (e) => {{
                        if (e.lengthComputable) {{
                            const percent = (e.loaded / e.total) * 100;
                            progressBar.style.width = percent + '%';
                        }}
                    }};
                    
                    xhr.onload = () => resolve();
                    xhr.onerror = () => reject();
                    xhr.send(file);
                }});
            }}
            
            progressBar.style.width = '0%';
            progressContainer.style.display = 'none';
            loadFiles(currentPath);
        }}

        async function rebootFactory() {{
            if (!confirm('Reboot back to the main Factory OS?')) return;
            await fetch('/api/reboot_factory', {{ method: 'POST' }});
            setTimeout(() => location.reload(), 2000);
        }}

        loadFiles();
    </script>
</body>
</html>
"#,
        ssid = ssid,
        ip = ip
    )
}
