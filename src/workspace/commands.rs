use gpui_kit::component::Root;
use gpui_kit::{App, Window};

pub(crate) fn dispatch_new_tab(window: &mut Window, cx: &mut App) {
    let Some(root) = window.root::<Root>() else {
        return;
    };
    let Some(tern_window) = root.and_then(|root| {
        root.read(cx)
            .view()
            .clone()
            .downcast::<crate::workspace::window::TernWindow>()
            .ok()
    }) else {
        return;
    };
    tern_window.update(cx, |view, cx| {
        view.open_new_browser_tab(window, cx);
    });
}

pub(crate) fn dispatch_new_tab_at(window: &mut Window, dir: &str, cx: &mut App) {
    let Some(root) = window.root::<Root>() else {
        return;
    };
    let Some(tern_window) = root.and_then(|root| {
        root.read(cx)
            .view()
            .clone()
            .downcast::<crate::workspace::window::TernWindow>()
            .ok()
    }) else {
        return;
    };
    tern_window.update(cx, |view, cx| {
        view.open_new_browser_tab_at(dir.to_string(), window, cx);
    });
}

pub(crate) fn dispatch_new_window(dir: &str, cx: &mut App) {
    let settings = crate::settings::load_settings_from_disk()
        .unwrap_or_else(|_| crate::settings::SettingsFile::default());
    let _ = crate::workspace::windows::open_tern_window_at(&settings, dir.to_string(), cx);
}
