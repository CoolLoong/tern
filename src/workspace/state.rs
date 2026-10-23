use gpui_kit::{App, px};

pub(crate) struct WindowState {
    pub(crate) browser_windows: Vec<gpui_kit::WindowHandle<gpui_kit::component::Root>>,
    pub(crate) settings_window: Option<gpui_kit::WindowHandle<gpui_kit::component::Root>>,
    pub(crate) sidebar_visible: bool,
    pub(crate) sidebar_width: gpui_kit::Pixels,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            browser_windows: Vec::new(),
            settings_window: None,
            sidebar_visible: true,
            sidebar_width: px(240.0),
        }
    }
}

impl gpui_kit::Global for WindowState {}

impl WindowState {
    pub(crate) fn record_browser_window(
        &mut self,
        handle: gpui_kit::WindowHandle<gpui_kit::component::Root>,
    ) {
        self.browser_windows
            .retain(|window| window.window_id() != handle.window_id());
        self.browser_windows.push(handle);
    }
}

pub(crate) fn init(cx: &mut App) {
    cx.set_global(WindowState::default());
    cx.on_window_closed(|cx, id| {
        let state = cx.global_mut::<WindowState>();
        state
            .browser_windows
            .retain(|window| window.window_id() != id);
        if state
            .settings_window
            .is_some_and(|window| window.window_id() == id)
        {
            state.settings_window = None;
        }
    })
    .detach();
}
