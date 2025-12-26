use std::fs::File;
use std::io::Write as StdWrite;
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
use esp_idf_svc::wifi::{AccessPointConfiguration, AuthMethod, BlockingWifi, Configuration, EspWifi};
use log::{error, info};

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

pub fn start_ap_file_server(modem: Modem, sd_root: Option<PathBuf>) -> WifiStateHandle {
    let state = Arc::new(Mutex::new(WifiState {
        mode: WifiMode::AccessPoint,
        ssid: "Cardputer-RustOS".to_string(),
        ip: None,
    }));

    let state_thread = state.clone();
    thread::spawn(move || {
        if let Err(err) = bringup_ap_and_server(modem, sd_root, state_thread) {
            error!("WiFi file server failed: {:?}", err);
        }
    });

    state
}

type ServerResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn bringup_ap_and_server(
    modem: Modem,
    sd_root: Option<PathBuf>,
    state: WifiStateHandle,
) -> ServerResult<()> {
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?;

    let ssid = "Cardputer-RustOS";
    let password = "cardputer";
    let ap_cfg = AccessPointConfiguration {
        ssid: ssid.try_into().unwrap(),
        password: password.try_into().unwrap(),
        channel: 6,
        auth_method: AuthMethod::WPA2Personal,
        max_connections: 4,
        ..Default::default()
    };

    wifi.set_configuration(&Configuration::AccessPoint(ap_cfg))?;
    wifi.start()?;
    wifi.wait_netif_up()?;

    if let Ok(ip_info) = wifi.wifi().ap_netif().get_ip_info() {
        let mut guard = state.lock().unwrap();
        guard.ip = Some(ip_info.ip.to_string());
    }

    info!("WiFi AP {} started", ssid);

    launch_http(sd_root, state.clone())?;

    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

fn launch_http(sd_root: Option<PathBuf>, state: WifiStateHandle) -> ServerResult<()> {
    let mut server = EspHttpServer::new(&HttpConfig {
        http_port: 8080,
        ..Default::default()
    })?;

    let index_body = render_index(&state);
    server.fn_handler("/", Method::Get, move |req| {
        let mut resp = req.into_ok_response()?;
        resp.write_all(index_body.as_bytes())?;
        Ok(())
    })?;

    let upload_root = sd_root.clone();
    server.fn_handler("/upload", Method::Post, move |mut req| {
        if upload_root.is_none() {
            let mut resp = req.into_ok_response()?;
            resp.write_all(b"SD card not mounted")?;
            return Ok(());
        }

        let filename = req
            .header("X-Filename")
            .map(str::to_owned)
            .unwrap_or_else(|| "upload.bin".to_string());

        let target = upload_root
            .as_ref()
            .map(|root| root.join(&filename))
            .unwrap_or_else(|| PathBuf::from(filename.clone()));

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = File::create(&target)?;
        let mut buf = [0u8; 1024];
        loop {
            let read = req.read(&mut buf)?;
            if read == 0 {
                break;
            }
            file.write_all(&buf[..read])?;
        }

        let mut resp = req.into_ok_response()?;
        resp.write_all(b"OK")?;
        Ok(())
    })?;

    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(60));
        let _ = &server;
    });

    Ok(())
}

fn render_index(state: &WifiStateHandle) -> String {
    let guard = state.lock().ok();
    let (ssid, ip) = guard
        .as_deref()
        .map(|s| (s.ssid.clone(), s.ip.clone().unwrap_or_else(|| "".to_string())))
        .unwrap_or_else(|| ("Cardputer-RustOS".to_string(), String::new()));

    format!(
        r#"<!doctype html>
<html>
<head><title>Cardputer RustOS</title></head>
<body>
<h1>Cardputer RustOS File Drop</h1>
<p>SSID: {ssid}</p>
<p>IP: {ip}</p>
<form id="uploadForm">
  <input type="file" id="file" name="file" />
  <input type="text" id="name" placeholder="filename on SD (optional)" />
  <button type="submit">Upload</button>
</form>
<pre id="status"></pre>
<script>
  const form = document.getElementById('uploadForm');
  form.addEventListener('submit', async (e) => {{
    e.preventDefault();
    const fileInput = document.getElementById('file');
    if (!fileInput.files.length) {{
      alert('Pick a file');
      return;
    }}
    const nameField = document.getElementById('name');
    const filename = nameField.value || fileInput.files[0].name;
    const resp = await fetch('/upload', {{
      method: 'POST',
      headers: {{ 'X-Filename': filename }},
      body: fileInput.files[0]
    }});
    const text = await resp.text();
    document.getElementById('status').innerText = 'Upload result: ' + text;
  }});
</script>
</body>
</html>
"#,
        ssid = ssid,
        ip = ip
    )
}
