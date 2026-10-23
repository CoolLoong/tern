use gpui_common::TermuaIcon;
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    Context, EventEmitter, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    StyledImage, Window, div, img, px,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDragEntry {
    pub id: String,
    pub name: String,
    pub is_dir: bool,
}

impl LocalDragEntry {
    pub fn new(id: impl Into<String>, name: impl Into<String>, is_dir: bool) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            is_dir,
        }
    }
}

impl EventEmitter<()> for LocalDragEntry {}

impl Render for LocalDragEntry {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let icon = if self.is_dir {
            TermuaIcon::FolderClosedBlue
        } else {
            TermuaIcon::FileText
        };
        div()
            .id("tern-local-drag")
            .cursor_grab()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(10.0))
            .py(px(6.0))
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .bg(cx.theme().popover)
            .child(
                img(icon)
                    .w(px(16.0))
                    .h(px(16.0))
                    .object_fit(gpui_kit::ObjectFit::Contain),
            )
            .child(self.name.clone())
    }
}
