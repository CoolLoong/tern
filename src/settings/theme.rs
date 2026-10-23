use super::config::{SettingsFile, Theme};
use gpui_kit::App;

pub fn set_theme_mode(mode: Theme, window: Option<&mut gpui_kit::Window>, cx: &mut App) {
    use gpui_kit::component::{Theme as AppTheme, ThemeMode};
    let desired = match mode {
        Theme::System => window
            .as_ref()
            .map(|window| window.appearance())
            .unwrap_or_else(|| cx.window_appearance())
            .into(),
        Theme::Light => ThemeMode::Light,
        Theme::Dark => ThemeMode::Dark,
    };
    if cx.has_global::<AppTheme>() && AppTheme::global(cx).mode == desired {
        return;
    }
    AppTheme::change(desired, window, cx);
}

impl SettingsFile {
    pub fn apply_to_app(&self, window: Option<&mut gpui_kit::Window>, cx: &mut App) {
        set_theme_mode(self.appearance.theme, window, cx);
    }
}
