use gpui_dock::{Panel, PanelEvent, PanelInfo, PanelState, TabIcon};
use gpui_kit::component::{Icon, IconName, button::ButtonVariants};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Subscription, Window, div,
};

use crate::browser::view::BrowserView;
pub(crate) const BROWSER_PANEL_NAME: &str = "tern.browser_panel";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct BrowserPanelState {
    pub(crate) version: usize,
    pub(crate) tab_label: String,
    pub(crate) current_dir: Option<String>,
}

pub struct BrowserDockPanel {
    pub(crate) tab_label: gpui_kit::SharedString,
    pub(crate) focus_handle: FocusHandle,
    browser_view: Option<Entity<BrowserView>>,
    restored_current_dir: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl BrowserDockPanel {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_at(None, window, cx)
    }

    pub(crate) fn new_at(
        initial_dir: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let browser_view = cx.new(|cx| BrowserView::new_at(initial_dir, window, cx));
        let mut this = Self {
            tab_label: "Home".into(),
            focus_handle,
            browser_view: None,
            restored_current_dir: None,
            _subscriptions: Vec::new(),
        };
        this.attach_browser_view(browser_view, window, cx);
        this
    }

    pub(crate) fn restoring(state: BrowserPanelState, cx: &mut Context<Self>) -> Self {
        Self {
            tab_label: state.tab_label.into(),
            focus_handle: cx.focus_handle(),
            browser_view: None,
            restored_current_dir: state.current_dir,
            _subscriptions: Vec::new(),
        }
    }

    pub(crate) fn browser_view(&self) -> Option<Entity<BrowserView>> {
        self.browser_view.clone()
    }

    fn attach_browser_view(
        &mut self,
        browser_view: Entity<BrowserView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._subscriptions.push(cx.observe_in(
            &browser_view,
            window,
            |this, view, _window, cx| {
                let status = view.read(cx).status(cx);
                crate::workspace::status::update_status(cx.entity_id(), status, cx);
                if let Some(dir) = view.read(cx).current_dir(cx) {
                    this.tab_label = crate::files::model::display_name_for_dir(&dir).into();
                    cx.notify();
                }
            },
        ));
        let focus = browser_view.read(cx).focus_handle(cx);
        let focused_browser_view = browser_view.clone();
        self._subscriptions
            .push(cx.on_focus_in(&focus, window, move |_this, _window, cx| {
                let status = focused_browser_view.read(cx).status(cx);
                crate::workspace::status::update_status(cx.entity_id(), status, cx);
            }));
        self.browser_view = Some(browser_view);
    }

    pub(crate) fn connect_browser_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let initial_dir = self.restored_current_dir.take();
        let browser_view = cx.new(|cx| BrowserView::new_at(initial_dir, window, cx));
        self.attach_browser_view(browser_view, window, cx);
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for BrowserDockPanel {}

impl Focusable for BrowserDockPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for BrowserDockPanel {
    fn inner_padding(&self, _cx: &App) -> bool {
        // The browser owns its toolbar spacing; Dock's multi-tab padding
        // would otherwise add an extra strip above the path bar.
        false
    }

    fn panel_name(&self) -> &'static str {
        BROWSER_PANEL_NAME
    }

    fn persistable(&self, _cx: &App) -> bool {
        true
    }

    fn tab_icon(&self, _cx: &App) -> Option<TabIcon> {
        Some(TabIcon::ColoredSvg {
            path: gpui_common::TermuaIcon::FolderClosedBlue.into(),
        })
    }

    fn tab_name(&self, _cx: &App) -> Option<gpui_kit::SharedString> {
        Some(self.tab_label.clone())
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(self.tab_label.clone())
    }

    fn title_prefix(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let browser_view = self.browser_view.as_ref()?.clone();
        Some(browser_view.read(cx).nav_controls(&browser_view, cx))
    }

    fn title_suffix(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        // Same recipe as the browser toolbar: only the icon, the action and the
        // position differ.
        Some(
            crate::browser::view::BrowserView::toolbar_button(
                ("browser-new-tab", cx.entity_id()),
                Icon::new(IconName::Plus),
                "New Tab",
            )
            .custom(crate::browser::view::BrowserView::toolbar_variant(cx))
            .tooltip_with_action("New Tab", &crate::app::actions::NewTab, None)
            .on_click(|_, window, cx| crate::workspace::commands::dispatch_new_tab(window, cx)),
        )
    }

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if active && let Some(browser_view) = &self.browser_view {
            let status = browser_view.read(cx).status(cx);
            crate::workspace::status::update_status(cx.entity_id(), status, cx);
            let focus = browser_view.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
    }

    fn dump(&self, cx: &App) -> PanelState {
        let current_dir = self
            .browser_view
            .as_ref()
            .and_then(|v| v.read(cx).current_dir(cx))
            .or_else(|| self.restored_current_dir.clone());
        let mut state = PanelState::new(self);
        state.info = PanelInfo::panel(
            serde_json::to_value(BrowserPanelState {
                version: 1,
                tab_label: self.tab_label.to_string(),
                current_dir,
            })
            .expect("browser panel state should serialize"),
        );
        state
    }
}

impl Render for BrowserDockPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.browser_view.is_none() {
            self.connect_browser_view(window, cx);
        }
        let Some(browser_view) = &self.browser_view else {
            return div().size_full();
        };
        div()
            .size_full()
            .track_focus(&self.focus_handle(cx))
            .child(browser_view.clone())
    }
}

#[cfg(test)]
mod chooser_tests {
    use super::*;
    #[gpui_kit::test]
    fn browser_tabs_do_not_add_padding_above_the_path_bar(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            crate::app::runtime::init_app(cx, &crate::settings::SettingsFile::default())
        });
        let (tabs, cx) = cx.add_window_view(|window, cx| {
            let dock = cx.new(|cx| gpui_dock::DockArea::new("test", None, window, cx));
            gpui_dock::TabPanel::new(None, dock.downgrade(), window, cx)
        });
        for _ in 0..2 {
            cx.update(|window, cx| {
                let panel: std::sync::Arc<dyn gpui_dock::PanelView> =
                    std::sync::Arc::new(cx.new(|cx| {
                        BrowserDockPanel::restoring(
                            BrowserPanelState {
                                version: 1,
                                tab_label: "test".into(),
                                current_dir: Some(
                                    std::env::temp_dir().to_string_lossy().into_owned(),
                                ),
                            },
                            cx,
                        )
                    }));
                tabs.update(cx, |tabs, cx| tabs.add_panel(panel, window, cx));
                assert!(!Panel::inner_padding(tabs.read(cx), cx));
            });
        }
    }
}
