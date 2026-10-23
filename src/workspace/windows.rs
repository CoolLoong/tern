use gpui_kit::component::TitleBar;
use gpui_kit::{App, AppContext, WindowBounds, WindowDecorations, WindowOptions, px, size};

pub(crate) fn open_tern_window(
    settings: &crate::settings::SettingsFile,
    cx: &mut App,
) -> gpui_kit::WindowHandle<gpui_kit::component::Root> {
    open_window(settings, None, true, Vec::new(), cx)
}

pub(crate) fn open_tern_window_at(
    settings: &crate::settings::SettingsFile,
    dir: String,
    cx: &mut App,
) -> gpui_kit::WindowHandle<gpui_kit::component::Root> {
    open_window(settings, Some(dir), false, Vec::new(), cx)
}

pub(crate) fn open_window(
    settings: &crate::settings::SettingsFile,
    dir: Option<String>,
    restore_workspace: bool,
    selected: Vec<String>,
    cx: &mut App,
) -> gpui_kit::WindowHandle<gpui_kit::component::Root> {
    let settings_for_window = settings.clone();
    let handle = cx
        .open_window(
            WindowOptions {
                app_id: Some("tern".into()),
                window_bounds: Some(WindowBounds::centered(size(px(1500.0), px(900.0)), cx)),
                titlebar: Some(TitleBar::title_bar_options()),
                window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
                window_min_size: Some(size(px(900.0), px(600.0))),
                kind: gpui_kit::WindowKind::Normal,
                ..Default::default()
            },
            move |window, cx| {
                settings_for_window.apply_to_app(Some(window), cx);
                let view = if restore_workspace {
                    cx.new(|cx| crate::workspace::window::TernWindow::new(window, cx))
                } else {
                    cx.new(|cx| {
                        crate::workspace::window::TernWindow::new_at(
                            dir.expect("directory"),
                            selected,
                            window,
                            cx,
                        )
                    })
                };
                cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
            },
        )
        .unwrap();
    cx.global_mut::<crate::workspace::state::WindowState>()
        .record_browser_window(handle);
    handle
}
