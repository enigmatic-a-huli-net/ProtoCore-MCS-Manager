//! Фоновый замер качества интернета: раз в 2 секунды DNS-пакет к 1.1.1.1 и 8.8.8.8.

use std::sync::Mutex;
use std::time::Duration;

use pcm_core::network::{self, History, Sample, Stats};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

pub struct NetMon {
    pub hist: Mutex<History>,
}

impl Default for NetMon {
    fn default() -> Self {
        // час замеров
        Self { hist: Mutex::new(History::new(1800)) }
    }
}

#[derive(Serialize)]
pub struct NetReport {
    pub enabled: bool,
    pub targets: Vec<String>,
    pub last_min: Stats,
    pub last_10min: Stats,
    pub last_hour: Stats,
    /// последние 10 минут для графика
    pub samples: Vec<Sample>,
}

pub fn report(app: &AppHandle) -> NetReport {
    let enabled = app.state::<AppState>().config.lock().unwrap().network_monitor;
    let nm = app.state::<NetMon>();
    let h = nm.hist.lock().unwrap();
    NetReport {
        enabled,
        targets: network::PROBE_TARGETS.iter().map(|s| s.to_string()).collect(),
        last_min: network::stats(h.last(60)),
        last_10min: network::stats(h.last(600)),
        last_hour: network::stats(h.last(3600)),
        samples: h.last(600).cloned().collect(),
    }
}

pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("pcm-netmon".into())
        .spawn(move || {
            let targets: Vec<String> = network::PROBE_TARGETS.iter().map(|s| s.to_string()).collect();
            loop {
                let on = app.state::<AppState>().config.lock().unwrap().network_monitor;
                if on {
                    let s = network::sample(&targets);
                    app.state::<NetMon>().hist.lock().unwrap().push(s);
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .ok();
}
