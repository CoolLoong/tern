use gpui_dock::{Panel, PanelEvent, PanelState, TabIcon};
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable};
use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Styled, Window, div, prelude::FluentBuilder, px,
};

pub const PLACES_PANEL_NAME: &str = "tern.places_sidebar";

#[derive(Clone, Copy)]
enum ComputerPlace {
    Home,
    Filesystem,
    Trash,
    Recent,
}

impl ComputerPlace {
    fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Filesystem => "Filesystem",
            Self::Trash => "Trash",
            Self::Recent => "Recent",
        }
    }

    fn path(self) -> Option<String> {
        match self {
            Self::Home => home::home_dir().map(|path| path.to_string_lossy().to_string()),
            Self::Filesystem => Some("/".into()),
            Self::Trash => Some(
                crate::files::operations::trash_files_dir()
                    .to_string_lossy()
                    .to_string(),
            ),
            Self::Recent => Some(crate::files::recent::RECENT_URI.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub enum PlacesEvent {
    Navigate(String),
}

pub struct PlacesSidebar {
    focus_handle: FocusHandle,
    computer_open: bool,
    bookmarks_open: bool,
}

impl PlacesSidebar {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            computer_open: true,
            bookmarks_open: true,
        }
    }

    fn section_header(
        id: &'static str,
        label: &'static str,
        open: bool,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let panel = cx.entity();
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(4.0))
            .py(px(3.0))
            .cursor_pointer()
            .hover(|this| this.bg(cx.theme().list_hover.opacity(0.4)))
            .on_mouse_down(gpui_kit::MouseButton::Left, move |_event, _window, cx| {
                panel.update(cx, |this, cx| on_click(this, cx));
            })
            .child(
                Icon::new(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .xsmall(),
            )
            .child(div().text_sm().child(label))
    }
}

impl EventEmitter<PlacesEvent> for PlacesSidebar {}
impl EventEmitter<PanelEvent> for PlacesSidebar {}

impl Focusable for PlacesSidebar {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for PlacesSidebar {
    fn panel_name(&self) -> &'static str {
        PLACES_PANEL_NAME
    }

    fn persistable(&self, _cx: &App) -> bool {
        false
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        Some("Places".into())
    }

    fn tab_icon(&self, _cx: &App) -> Option<TabIcon> {
        Some(TabIcon::ColoredSvg {
            path: gpui_common::TermuaIcon::FolderTree.into(),
        })
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Places")
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState::new(self)
    }
}

impl Render for PlacesSidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let panel = cx.entity();
        let computer_open = self.computer_open;
        let bookmarks_open = self.bookmarks_open;

        div()
            .id("places-sidebar")
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle(cx))
            .p(px(4.0))
            .gap(px(0.0))
            .child(Self::section_header(
                "places-my-computer",
                "My Computer",
                computer_open,
                |this, cx| {
                    this.computer_open = !this.computer_open;
                    cx.notify();
                },
                cx,
            ))
            .when(computer_open, |this| {
                this.children(
                    [
                        ComputerPlace::Home,
                        ComputerPlace::Filesystem,
                        ComputerPlace::Trash,
                        ComputerPlace::Recent,
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(index, place)| {
                        let path = place.path();
                        let has_path = path.is_some();
                        let panel = panel.clone();
                        div()
                            .id(("computer-place", index))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .pl(px(22.0))
                            .pr(px(4.0))
                            .py(px(3.0))
                            .rounded_sm()
                            .when(has_path, |this| {
                                this.cursor_pointer()
                                    .hover(|this| this.bg(cx.theme().list_hover.opacity(0.4)))
                            })
                            .on_mouse_down(
                                gpui_kit::MouseButton::Left,
                                move |_event, _window, cx| {
                                    if let Some(path) = &path {
                                        panel.update(cx, |_, cx| {
                                            cx.emit(PlacesEvent::Navigate(path.clone()))
                                        });
                                    }
                                },
                            )
                            .child(match place {
                                ComputerPlace::Trash => Icon::default()
                                    .path(gpui_common::TermuaIcon::Trash)
                                    .small()
                                    .text_color(muted)
                                    .into_any_element(),
                                _ => Icon::new(IconName::Folder)
                                    .small()
                                    .text_color(muted)
                                    .into_any_element(),
                            })
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if has_path {
                                        cx.theme().foreground
                                    } else {
                                        muted
                                    })
                                    .child(place.label()),
                            )
                    }),
                )
            })
            .child(Self::section_header(
                "places-bookmarks",
                "Bookmarks",
                bookmarks_open,
                |this, cx| {
                    this.bookmarks_open = !this.bookmarks_open;
                    cx.notify();
                },
                cx,
            ))
            .when(bookmarks_open, |this| {
                let bookmarks = cx
                    .global::<crate::navigation::bookmarks::Bookmarks>()
                    .items()
                    .to_vec();
                if bookmarks.is_empty() {
                    this.child(
                        div()
                            .pl(px(22.0))
                            .py(px(3.0))
                            .text_sm()
                            .text_color(muted)
                            .child("No bookmarks"),
                    )
                } else {
                    this.children(bookmarks.into_iter().enumerate().map(|(index, path)| {
                        let panel = panel.clone();
                        let label = crate::files::model::display_name_for_dir(&path);
                        div()
                            .id(("bookmark-place", index))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .pl(px(22.0))
                            .pr(px(4.0))
                            .py(px(3.0))
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|this| this.bg(cx.theme().list_hover.opacity(0.4)))
                            .on_mouse_down(
                                gpui_kit::MouseButton::Left,
                                move |_event, _window, cx| {
                                    panel.update(cx, |_, cx| {
                                        cx.emit(PlacesEvent::Navigate(path.clone()));
                                    });
                                },
                            )
                            .child(Icon::new(IconName::Folder).small().text_color(muted))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().foreground)
                                    .child(label),
                            )
                    }))
                }
            })
    }
}
