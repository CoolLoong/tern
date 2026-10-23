use gpui_kit::{
    App, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled, Window,
    div,
};

pub(crate) fn build(window: &mut Window, cx: &mut App) -> gpui_kit::component::TitleBar {
    let mut tb = gpui_kit::component::TitleBar::new();
    let state = window.use_state(cx, |window, cx| MenubarTitleBarState {
        menubar: menubar::FoldableAppMenuBar::new(window, cx),
    });
    tb =
        tb.child(state.read(cx).menubar.clone())
            .child(div().h_full().flex_1().on_mouse_down(
                gpui_kit::MouseButton::Left,
                |_, _, cx| {
                    gpui_kit::component::GlobalState::suppress_text_selection(cx);
                },
            ));
    tb
}

struct MenubarTitleBarState {
    menubar: Entity<menubar::FoldableAppMenuBar>,
}

impl Render for MenubarTitleBarState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
