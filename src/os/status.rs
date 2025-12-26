use std::time::{Duration, Instant, UNIX_EPOCH};

use esp_idf_svc::systime::EspSystemTime;

use super::web::{WifiMode, WifiStateHandle};

#[derive(Clone, Debug, Default)]
pub struct StatusSnapshot {
    pub clock_text: String,
    pub wifi_text: String,
    pub battery_text: String,
}

pub struct StatusProvider {
    wifi: WifiStateHandle,
    battery: BatteryGauge,
    started_at: Instant,
}

impl StatusProvider {
    pub fn new(wifi: WifiStateHandle, battery: BatteryGauge) -> Self {
        Self {
            wifi,
            battery,
            started_at: Instant::now(),
        }
    }

    pub fn snapshot(&self) -> StatusSnapshot {
        StatusSnapshot {
            clock_text: self.clock_text(),
            wifi_text: self.wifi_text(),
            battery_text: self.battery_text(),
        }
    }

    fn clock_text(&self) -> String {
        match EspSystemTime {}.now().duration_since(UNIX_EPOCH) {
            Ok(duration) => format_hms(duration),
            Err(_) => format_hms(Duration::from_secs(self.started_at.elapsed().as_secs())),
        }
    }

    fn wifi_text(&self) -> String {
        let state = self.wifi.lock().ok();
        if let Some(state) = state.as_deref() {
            match state.mode {
                WifiMode::AccessPoint => {
                    let ip = state.ip.clone().unwrap_or_else(|| "...".to_string());
                    format!("AP {} @ {}", state.ssid, ip)
                }
                WifiMode::Station => {
                    let ip = state.ip.clone().unwrap_or_else(|| "...".to_string());
                    format!("WiFi {} @ {}", state.ssid, ip)
                }
            }
        } else {
            "WiFi offline".to_string()
        }
    }

    fn battery_text(&self) -> String {
        match self.battery.percent() {
            Some(pct) => format!("Batt {}%", pct),
            None => "Batt --%".to_string(),
        }
    }
}

fn format_hms(duration: Duration) -> String {
    let total_seconds = duration.as_secs() % 86_400;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

#[derive(Clone, Debug, Default)]
pub struct BatteryGauge;

impl BatteryGauge {
    pub fn new() -> Self {
        Self
    }

    pub fn percent(&self) -> Option<u8> {
        Some(100)
    }
}
