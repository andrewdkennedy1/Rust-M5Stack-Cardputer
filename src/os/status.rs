use std::time::{Duration, Instant};

use core::fmt::Write;

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
    last_update: Instant,
    snapshot: StatusSnapshot,
}

impl StatusProvider {
    pub fn new(wifi: WifiStateHandle, battery: BatteryGauge) -> Self {
        let mut provider = Self {
            wifi,
            battery,
            started_at: Instant::now(),
            last_update: Instant::now(),
            snapshot: StatusSnapshot::default(),
        };
        provider.refresh_snapshot();
        provider
    }

    pub fn snapshot(&mut self) -> &StatusSnapshot {
        if self.last_update.elapsed() >= Duration::from_millis(500) {
            self.refresh_snapshot();
            self.last_update = Instant::now();
        }
        &self.snapshot
    }

    fn refresh_snapshot(&mut self) {
        self.snapshot.clock_text.clear();
        let duration = EspSystemTime.now();
        if duration.as_secs() == 0 {
            write_hms(&mut self.snapshot.clock_text, self.started_at.elapsed());
        } else {
            write_hms(&mut self.snapshot.clock_text, duration);
        }
        self.snapshot.wifi_text.clear();
        if let Ok(state) = self.wifi.lock() {
            match state.mode {
                WifiMode::AccessPoint => {
                    if let Some(ip) = state.ip.as_deref() {
                        let _ = write!(self.snapshot.wifi_text, "AP {} @ {}", state.ssid, ip);
                    } else {
                        let _ = write!(self.snapshot.wifi_text, "AP {}", state.ssid);
                    }
                }
                WifiMode::Station => {
                    if let Some(ip) = state.ip.as_deref() {
                        let _ = write!(self.snapshot.wifi_text, "WiFi {} @ {}", state.ssid, ip);
                    } else {
                        let _ = write!(self.snapshot.wifi_text, "WiFi {}", state.ssid);
                    }
                }
            }
        } else {
            self.snapshot.wifi_text.push_str("WiFi offline");
        }

        self.snapshot.battery_text.clear();
        match self.battery.percent() {
            Some(pct) => {
                let _ = write!(self.snapshot.battery_text, "Batt {}%", pct);
            }
            None => self.snapshot.battery_text.push_str("Batt --%"),
        }
    }
}

fn write_hms(out: &mut String, duration: Duration) {
    let total_seconds = duration.as_secs() % 86_400;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    let _ = write!(out, "{:02}:{:02}:{:02}", hours, minutes, seconds);
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
