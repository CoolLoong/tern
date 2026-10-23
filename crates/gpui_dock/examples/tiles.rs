use std::sync::Arc;

use gpui_dock::{DockArea, DockItem, Panel, PanelEvent, PanelView, TileMeta};
use gpui_kit::assets::Assets;
use gpui_kit::component::Root;
use gpui_kit::{
    App, Bounds, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Window, WindowOptions,
    div, point, prelude::*, px, size,
};

struct DemoPanel {
    focus: FocusHandle,
    label: gpui_kit::SharedString,
}

impl DemoPanel {
    fn new(label: impl Into<gpui_kit::SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            label: label.into(),
        }
    }
}

impl EventEmitter<PanelEvent> for DemoPanel {}

impl Focusable for DemoPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Panel for DemoPanel {
    fn panel_name(&self) -> &'static str {
        "gpui_dock.example.demo_panel"
    }

    fn tab_name(&self, _cx: &App) -> Option<gpui_kit::SharedString> {
        Some(self.label.clone())
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(self.label.clone())
    }
}

impl gpui_kit::Render for DemoPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .track_focus(&self.focus_handle(cx))
            .p(gpui_kit::px(10.))
            .child(format!("Demo panel: {}", self.label))
    }
}

struct MainView {
    dock_area: gpui_kit::Entity<DockArea>,
}

impl gpui_kit::Render for MainView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.dock_area.clone())
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(Assets)
        .run(|cx: &mut App| {
            gpui_kit::component::init(cx);
            gpui_dock::init(cx);
            cx.activate(true);

            cx.open_window(WindowOptions::default(), |window, cx| {
                let dock_area = cx.new(|cx| DockArea::new("dock", Some(1), window, cx));
                let weak = dock_area.downgrade();

                let a: Arc<dyn PanelView> = Arc::new(cx.new(|cx| DemoPanel::new("Floating A", cx)));
                let b: Arc<dyn PanelView> = Arc::new(cx.new(|cx| DemoPanel::new("Floating B", cx)));

                dock_area.update(cx, |area, cx| {
                    // tiles() only uses DockItem::Panel and DockItem::Tabs; other variants are
                    // ignored.
                    let tiles = DockItem::tiles(
                        vec![
                            DockItem::panel(a),
                            DockItem::tabs(vec![b], &weak, window, cx),
                        ],
                        vec![
                            TileMeta {
                                bounds: Bounds {
                                    origin: point(px(20.), px(20.)),
                                    size: size(px(560.), px(320.)),
                                },
                                z_index: 1,
                            },
                            Bounds {
                                origin: point(px(120.), px(380.)),
                                size: size(px(520.), px(260.)),
                            }
                            .into(),
                        ],
                        &weak,
                        window,
                        cx,
                    );

                    area.set_center(tiles, window, cx);
                    area.update_toggle_button_tab_panels(window, cx);
                });

                let view = cx.new(|_cx| MainView { dock_area });
                cx.new(|cx| Root::new(view, window, cx))
            })
            .unwrap();
        });
}
