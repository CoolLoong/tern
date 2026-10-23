//! Settings persistence, theme application and the GPUI settings window.

pub(crate) mod config;
pub(crate) mod theme;
pub(crate) mod window;

pub(crate) use config::{
    SettingsFile, Theme, config_dir_path, load_settings_from_disk, save_settings_to_disk,
    settings_json_path,
};
