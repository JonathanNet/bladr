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

fn paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|profile| PathBuf::from(profile).join("AppData").join("Roaming"))
        });
    if let Some(base) = base {
        paths.push(base.join("Bladr").join("settings.txt"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join("settings.txt"));
        }
    }
    paths
}

fn parse(text: &str, settings: &mut Settings) {
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
}

pub fn load() -> Settings {
    let mut settings = Settings::default();
    for path in paths() {
        if let Ok(text) = std::fs::read_to_string(path) {
            parse(&text, &mut settings);
            break;
        }
    }
    settings
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let text = format!(
        "auto_update={}\nstart_fullscreen={}\nlast_check={}\n",
        if settings.auto_update { 1 } else { 0 },
        if settings.start_fullscreen { 1 } else { 0 },
        settings.last_check
    );

    let mut saved = false;
    let mut last_error = "settings folder unavailable".to_owned();
    for path in paths() {
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                last_error = format!("create settings folder: {error}");
                continue;
            }
        }
        match std::fs::write(&path, &text) {
            Ok(()) => saved = true,
            Err(error) => last_error = format!("write settings: {error}"),
        }
    }
    if saved {
        Ok(())
    } else {
        Err(last_error)
    }
}
