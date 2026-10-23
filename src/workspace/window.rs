use super::status::StatusState;
use crate::browser::panel::{BROWSER_PANEL_NAME, BrowserPanelState};
use std::sync::Arc;

use gpui_dock::{DockArea, DockEvent, DockItem, DockPlacement, PanelView};
use gpui_kit::component::{ActiveTheme, WindowExt};
use gpui_kit::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};
use gpui_transfer::{TransferCenterState, TransferProgress};

use crate::app::actions::*;
use crate::workspace::state::WindowState;

pub struct TernWindow {
    dock_area: Entity<DockArea>,
    _subscriptions: Vec<Subscription>,
}

impl TernWindow {
    pub(crate) fn new_at(
        dir: String,
        selected: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::new_empty(window, cx);
        this.open_new_browser_tab_selected(dir, selected, window, cx);
        this
    }

    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self::new_empty(window, cx);

        let state_path = crate::workspace::persistence::state_path();
        match crate::workspace::persistence::load_from_path(&state_path) {
            Ok(mut state)
                if state.version == Some(crate::workspace::persistence::STATE_VERSION) =>
            {
                state.left_dock = None;
                this.dock_area.update(cx, |dock, cx| {
                    let _ = dock.load(state, window, cx);
                });
            }
            Ok(_) => log::debug!(
                "ignoring incompatible workspace state at {}",
                state_path.display()
            ),
            Err(err) if state_path.exists() => {
                log::warn!(
                    "ignoring unreadable workspace state at {}: {err:#}",
                    state_path.display()
                );
            }
            Err(_) => {}
        }

        let has_browser = this
            .dock_area
            .read(cx)
            .visible_tab_panels(cx)
            .into_iter()
            .flat_map(|tabs| tabs.read(cx).panels().iter().cloned())
            .any(|panel| {
                panel
                    .view()
                    .downcast::<crate::browser::panel::BrowserDockPanel>()
                    .is_ok()
            });
        if !has_browser {
            this.open_new_browser_tab(window, cx);
        }
        this
    }

    fn new_empty(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let dock_area = cx.new(|cx| DockArea::new("tern", Some(1), window, cx));
        let dock_weak = dock_area.downgrade();

        dock_area.update(cx, |dock, cx| {
            let center = DockItem::v_split(
                vec![DockItem::tabs(vec![], &dock_weak, window, cx)],
                &dock_weak,
                window,
                cx,
            );
            dock.set_center(center, window, cx);
        });

        let places = cx.new(|cx| crate::navigation::sidebar::PlacesSidebar::new(window, cx));
        let nav_sub = cx.subscribe_in(&places, window, |this, _places, event, window, cx| {
            let crate::navigation::sidebar::PlacesEvent::Navigate(path) = event;
            this.navigate_active_tab(path.clone(), window, cx);
        });
        let sidebar = crate::workspace::sidebar_state::load_from_disk();
        {
            let app_state = cx.global_mut::<WindowState>();
            app_state.sidebar_width = gpui_kit::px(sidebar.width);
            app_state.sidebar_visible = sidebar.visible;
        }
        let places_panel: Arc<dyn PanelView> = Arc::new(places);
        dock_area.update(cx, |dock, cx| {
            let state = cx.global::<WindowState>();
            dock.set_left_dock(
                DockItem::panel(places_panel),
                Some(state.sidebar_width),
                state.sidebar_visible,
                window,
                cx,
            );
            if let Some(left) = dock.left_dock().cloned() {
                left.update(cx, |dock, cx| {
                    dock.set_min_size(px(180.0), window, cx);
                    dock.set_max_size(px(360.0), window, cx);
                });
            }
        });

        let mut this = Self {
            dock_area,
            _subscriptions: vec![
                nav_sub,
                // The footer reads StatusState, so it must repaint when the
                // count changes. Relying on a descendant's notify to redraw this
                // ancestor leaves the count stale whenever nothing else moved.
                cx.observe_global::<StatusState>(|_, cx| cx.notify()),
            ],
        };

        this._subscriptions
            .push(cx.observe_window_activation(window, |_, window, cx| {
                if window.is_window_active()
                    && let Some(handle) = window
                        .window_handle()
                        .downcast::<gpui_kit::component::Root>()
                {
                    cx.global_mut::<WindowState>().record_browser_window(handle);
                }
            }));

        let dock = this.dock_area.clone();
        let own_window = window.window_handle().window_id();
        this._subscriptions
            .push(cx.on_window_closed(move |cx, window_id| {
                if window_id != own_window {
                    return;
                }
                let state = dock.read(cx).dump(cx);
                if let Err(err) = crate::workspace::persistence::save_to_path(
                    &crate::workspace::persistence::state_path(),
                    state,
                ) {
                    log::warn!("failed to save workspace on close: {err:#}");
                }
            }));

        this._subscriptions.push(cx.subscribe_in(
            &this.dock_area,
            window,
            |this, _dock, event, _window, cx| {
                if matches!(event, DockEvent::LayoutChanged) {
                    this.save_sidebar_state(cx);
                }
            },
        ));

        this
    }

    pub(crate) fn open_new_browser_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = cx.new(|cx| crate::browser::panel::BrowserDockPanel::new(window, cx));
        let panel: Arc<dyn PanelView> = Arc::new(panel);
        self.dock_area.update(cx, |dock, cx| {
            dock.add_panel(panel, DockPlacement::Center, None, window, cx);
        });
        cx.notify();
    }

    pub(crate) fn open_new_browser_tab_at(
        &mut self,
        dir: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_new_browser_tab_selected(dir, Vec::new(), window, cx);
    }

    fn open_new_browser_tab_selected(
        &mut self,
        dir: String,
        selected: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel =
            cx.new(|cx| crate::browser::panel::BrowserDockPanel::new_at(Some(dir), window, cx));
        panel.update(cx, |panel, cx| {
            if !selected.is_empty()
                && let Some(view) = panel.browser_view()
            {
                view.update(cx, |view, cx| view.select_paths(selected, cx));
            }
        });
        let panel: Arc<dyn PanelView> = Arc::new(panel);
        self.dock_area.update(cx, |dock, cx| {
            dock.add_panel(panel, DockPlacement::Center, None, window, cx);
        });
        cx.notify();
    }

    fn on_new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("action: NewTab");
        self.open_new_browser_tab(window, cx);
    }

    fn on_new_window(&mut self, _: &NewWindow, _window: &mut Window, cx: &mut Context<Self>) {
        log::info!("action: NewWindow");
        let settings = crate::settings::load_settings_from_disk()
            .unwrap_or_else(|_| crate::settings::SettingsFile::default());
        crate::workspace::windows::open_tern_window(&settings, cx);
    }

    fn on_open_settings(&mut self, _: &OpenSettings, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(settings_window) = cx.global::<WindowState>().settings_window
            && settings_window
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        match crate::settings::window::SettingsWindow::open(cx) {
            Ok(window) => cx.global_mut::<WindowState>().settings_window = Some(window),
            Err(err) => log::warn!("failed to open settings: {err:#}"),
        }
    }

    fn on_about(&mut self, _: &About, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("action: About");
        let version = env!("CARGO_PKG_VERSION");
        window.open_alert_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .title(format!("Tern {version}"))
                .description("A fast native file manager built with Rust and GPUI.")
                .confirm()
        });
    }

    fn on_toggle_theme(&mut self, _: &ToggleTheme, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("action: ToggleTheme");
        let mut settings = crate::settings::load_settings_from_disk()
            .unwrap_or_else(|_| crate::settings::SettingsFile::default());
        settings.appearance.theme = match settings.appearance.theme {
            crate::settings::Theme::Dark => crate::settings::Theme::Light,
            crate::settings::Theme::Light => crate::settings::Theme::System,
            crate::settings::Theme::System => crate::settings::Theme::Dark,
        };
        settings.apply_to_app(Some(window), cx);
        if let Err(err) = crate::settings::save_settings_to_disk(&settings) {
            log::warn!("failed to save settings: {err:#}");
        }
    }

    pub(crate) fn save_workspace(&self, cx: &App) {
        let state = self.dock_area.read(cx).dump(cx);
        if let Err(err) = crate::workspace::persistence::save_to_path(
            &crate::workspace::persistence::state_path(),
            state,
        ) {
            log::warn!("failed to save workspace: {err:#}");
        }
    }

    fn save_sidebar_state(&mut self, cx: &mut App) {
        let Some(left) = self.dock_area.read(cx).left_dock() else {
            return;
        };
        let (width, visible) = {
            let left = left.read(cx);
            (left.size().as_f32(), left.is_open())
        };
        let state = crate::workspace::sidebar_state::SidebarState { width, visible };
        let app_state = cx.global_mut::<WindowState>();
        app_state.sidebar_width = gpui_kit::px(width);
        app_state.sidebar_visible = visible;
        if let Err(err) = crate::workspace::sidebar_state::save_to_disk(&state) {
            log::warn!("failed to save sidebar state: {err:#}");
        }
    }

    fn on_quit(&mut self, _: &Quit, _window: &mut Window, cx: &mut Context<Self>) {
        log::info!("action: Quit");
        self.save_workspace(cx);
        cx.quit();
    }

    fn on_close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        let active = self
            .dock_area
            .read(cx)
            .visible_tab_panels(cx)
            .into_iter()
            .next();
        if let Some(tab_panel) = active
            && let Some(panel) = tab_panel.read(cx).active_panel(cx)
        {
            tab_panel.update(cx, |tab_panel, cx| {
                tab_panel.remove_panel(panel, window, cx);
            });
        }
    }

    fn navigate_active_tab(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.active_browser_view(cx) else {
            self.open_new_browser_tab(window, cx);
            return;
        };
        view.update(cx, |view, cx| view.change_dir(path, cx));
    }

    fn on_toggle_sidebar(
        &mut self,
        _: &ToggleSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let visible = cx.global::<WindowState>().sidebar_visible;
        cx.global_mut::<WindowState>().sidebar_visible = !visible;
        if let Some(left) = self.dock_area.read(cx).left_dock().cloned() {
            left.update(cx, |dock, cx| dock.set_open(!visible, window, cx));
        }
        self.save_sidebar_state(cx);
    }

    fn active_browser_view(
        &self,
        cx: &Context<Self>,
    ) -> Option<Entity<crate::browser::view::BrowserView>> {
        let active = self
            .dock_area
            .read(cx)
            .visible_tab_panels(cx)
            .into_iter()
            .next()?;
        let panel = active.read(cx).active_panel(cx)?;
        let browser_panel = panel
            .view()
            .downcast::<crate::browser::panel::BrowserDockPanel>()
            .ok()?;
        browser_panel.read(cx).browser_view()
    }

    fn on_refresh(&mut self, _: &Refresh, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.active_browser_view(cx) else {
            return;
        };
        view.update(cx, |view, cx| view.refresh(window, cx));
    }

    fn on_toggle_hidden(
        &mut self,
        _: &ToggleHiddenFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.active_browser_view(cx) else {
            return;
        };
        view.update(cx, |view, cx| view.toggle_hidden(window, cx));
    }
}

impl Render for TernWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let titlebar = crate::workspace::titlebar::build(window, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_stretch()
            .on_action(cx.listener(Self::on_new_tab))
            .on_action(cx.listener(Self::on_new_window))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(Self::on_quit))
            .on_action(cx.listener(Self::on_open_settings))
            .on_action(cx.listener(Self::on_about))
            .on_action(cx.listener(Self::on_toggle_sidebar))
            .on_action(cx.listener(Self::on_toggle_theme))
            .on_action(cx.listener(Self::on_refresh))
            .on_action(cx.listener(Self::on_toggle_hidden))
            .child(div().child(titlebar))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .h_full()
                    .relative()
                    .child(self.dock_area.clone()),
            )
            .child(self.render_footbar(cx))
            .children(gpui_kit::component::Root::render_sheet_layer(window, cx))
            .children(gpui_kit::component::Root::render_dialog_layer(window, cx))
            .children(gpui_kit::component::Root::render_notification_layer(
                window, cx,
            ))
    }
}

impl TernWindow {
    fn render_footbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let state = cx.global::<TransferCenterState>();
        let summary = state.summary();
        // Left side: how much is on screen and how much is picked. Right side:
        // any operation in flight.
        let counts = {
            let status = cx.global::<StatusState>();
            let base = if status.selected > 0 {
                format!("{} of {} selected", status.selected, status.items)
            } else {
                format!("{} items", status.items)
            };
            // Say so when a search was cut short: otherwise the count reads as
            // "everything that matches".
            if status.searching {
                format!("{base} · searching…")
            } else if status.truncated {
                format!("{base} (first results)")
            } else {
                base
            }
        };
        let (popover, border) = {
            let t = cx.theme();
            (t.popover, t.border)
        };

        let transfers = if summary.total > 0 {
            let (bar_width, fill_width) = match summary.progress() {
                TransferProgress::Determinate(f) => (px(120.0), px(120.0 * f)),
                TransferProgress::Indeterminate => (px(120.0), px(0.0)),
            };
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} / {}", summary.done, summary.total)),
                    )
                    .child(
                        div()
                            .w(bar_width)
                            .rounded_sm()
                            .bg(cx.theme().muted.opacity(0.25))
                            .py(px(3.0))
                            .child(
                                div()
                                    .w(fill_width)
                                    .rounded_sm()
                                    .bg(cx.theme().selection)
                                    .py(px(3.0)),
                            ),
                    ),
            )
        } else {
            None
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(10.0))
            .py(px(6.0))
            .border_t_1()
            .border_color(border.opacity(0.8))
            .bg(popover.opacity(0.65))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(counts),
            )
            .when_some(transfers, |this, transfers| this.child(transfers))
    }
}

pub(crate) fn register_panel_factories(cx: &mut App) {
    gpui_dock::register_panel(
        cx,
        crate::navigation::sidebar::PLACES_PANEL_NAME,
        |_, _, _, window, cx| {
            Box::new(cx.new(|cx| crate::navigation::sidebar::PlacesSidebar::new(window, cx)))
        },
    );
    gpui_dock::register_panel(cx, BROWSER_PANEL_NAME, |_, _, info, _, cx| {
        let state = match info {
            gpui_dock::PanelInfo::Panel(value) => {
                serde_json::from_value::<BrowserPanelState>(value.clone())
            }
            _ => unreachable!("browser factory received non-panel state"),
        };
        match state {
            Ok(state) if state.version == 1 => {
                Box::new(cx.new(|cx| crate::browser::panel::BrowserDockPanel::restoring(state, cx)))
            }
            Ok(_) | Err(_) => Box::new(cx.new(|cx| {
                crate::browser::panel::BrowserDockPanel::restoring(
                    BrowserPanelState {
                        version: 1,
                        tab_label: "Home".into(),
                        current_dir: None,
                    },
                    cx,
                )
            })),
        }
    });
}
