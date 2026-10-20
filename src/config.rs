//! Configuration dans %APPDATA%\Nemuru\config.json

use crate::fade::FadeCurve;
use crate::power::SleepAction;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub duration_minutes: u64,
    pub fade_curve: FadeCurve,
    pub sleep_action: SleepAction,
    pub google_tv_enabled: bool,
    pub google_tv_ip: String,
    pub google_tv_port: u16,
    pub late_fade_minutes: u64,
    pub run_at_startup: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            duration_minutes: 45,
            fade_curve: FadeCurve::Logarithmic,
            sleep_action: SleepAction::Sleep,
            google_tv_enabled: false,
            google_tv_ip: String::new(),
            google_tv_port: 8009,
            late_fade_minutes: 10,
            run_at_startup: false,
        }
    }
}

impl AppConfig {
    pub fn dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Nemuru")
    }

    pub fn config_path() -> PathBuf {
        Self::dir().join("config.json")
    }

    pub fn log_path() -> PathBuf {
        Self::dir().join("nemuru.log")
    }

    pub fn load_or_default() -> Result<Self> {
        if let Some(cfg) = read_config(&Self::config_path())? {
            return Ok(cfg);
        }
        let legacy = legacy_config_path();
        if let Some(cfg) = read_config(&legacy)? {
            cfg.save()?;
            return Ok(cfg);
        }
        let default_cfg = Self::default();
        default_cfg.save()?;
        Ok(default_cfg)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.sanitized())?;
        fs::write(path, json)?;
        Ok(())
    }

    fn sanitized(&self) -> Self {
        let mut copy = self.clone();
        copy.duration_minutes = copy.duration_minutes.clamp(1, 480);
        if copy.late_fade_minutes == 0 {
            copy.late_fade_minutes = 10;
        }
        copy.late_fade_minutes = copy.late_fade_minutes.min(copy.duration_minutes).max(1);
        if copy.google_tv_port == 0 {
            copy.google_tv_port = 8009;
        }
        copy.google_tv_ip = copy.google_tv_ip.trim().to_string();
        copy
    }
}

fn legacy_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("SommeilDouceur")
        .join("config.json")
}

fn read_config(path: &Path) -> Result<Option<AppConfig>> {
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(path)?;
    match serde_json::from_str::<AppConfig>(&data) {
        Ok(cfg) => Ok(Some(cfg.sanitized())),
        Err(err) => {
            log::warn!("Configuration illisible, réglages par défaut : {err}");
            Ok(Some(AppConfig::default()))
        }
    }
}
