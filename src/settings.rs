//! Small persisted settings file. Unknown lines are ignored.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct Settings {
    pub auto_update: bool,
    pub start_fullscreen: bool,
    pub last_check: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_update: true,
            start_fullscreen: false,
            last_check: 0,
        }
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| {
        PathBuf::from(appdata)
            .join("Bladr")
            .join("settings.txt")
    })
}

pub fn load() -> Settings {
    let mut settings = Settings::default();
    let Some(path) = path() else {
        return settings;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return settings;
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "auto_update" => settings.auto_update = value.trim() != "0",
            "start_fullscreen" => settings.start_fullscreen = value.trim() != "0",
            "last_check" => settings.last_check = value.trim().parse().unwrap_or(0),
            _ => {}
        }
    }
    settings
}

pub fn save(settings: &Settings) {
    let Some(path) = path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = format!(
        "auto_update={}\nstart_fullscreen={}\nlast_check={}\n",
        if settings.auto_update { 1 } else { 0 },
        if settings.start_fullscreen { 1 } else { 0 },
        settings.last_check
    );
    let _ = std::fs::write(path, text);
}
