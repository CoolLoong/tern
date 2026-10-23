use std::{collections::BTreeMap, path::PathBuf};

use anyhow::Context as _;
use gpui_kit::SharedString;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "light")]
    Light,
    #[serde(rename = "dark")]
    Dark,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppearanceSettings {
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub light_theme: Option<SharedString>,
    #[serde(default)]
    pub dark_theme: Option<SharedString>,
    #[serde(default)]
    pub show_hidden_files: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SettingsFile {
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub keybindings: BTreeMap<String, String>,
}

pub fn config_dir_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("tern");
    }
    home::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("tern")
}

pub fn settings_json_path() -> PathBuf {
    config_dir_path().join("settings.json")
}

impl SettingsFile {
    pub fn load_from_path(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let json = std::fs::read_to_string(path)
            .with_context(|| format!("read settings {}", path.display()))?;
        let parsed: SettingsFile = serde_json::from_str(&json).context("deserialize settings")?;
        Ok(parsed)
    }

    pub fn save_to_path(&self, path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create settings dir {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self).context("serialize settings")?;
        std::fs::write(path, json).with_context(|| format!("write settings {}", path.display()))
    }
}

pub fn load_settings_from_disk() -> anyhow::Result<SettingsFile> {
    let path = settings_json_path();
    if !path.exists() {
        return Ok(SettingsFile::default());
    }
    SettingsFile::load_from_path(path)
}

pub fn save_settings_to_disk(settings: &SettingsFile) -> anyhow::Result<()> {
    settings.save_to_path(settings_json_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_path(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tern-settings-{label}-{nanos}.json"))
    }

    #[test]
    fn settings_round_trips() {
        let path = unique_path("roundtrip");
        let settings = SettingsFile {
            appearance: AppearanceSettings {
                theme: Theme::Dark,
                ..Default::default()
            },
            ..Default::default()
        };
        settings.save_to_path(&path).unwrap();
        let loaded = SettingsFile::load_from_path(&path).unwrap();
        assert_eq!(loaded.appearance.theme, Theme::Dark);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn missing_settings_defaults_to_light_system() {
        let path = unique_path("missing");
        assert!(SettingsFile::load_from_path(&path).is_err());
    }
}
