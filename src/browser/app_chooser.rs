use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::platform::open_with::{AppEntry, AppOpener, FileInfo};
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, WindowExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
};
use gpui_kit::{
    AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};

#[derive(Clone, Copy)]
pub(crate) enum AppChooserEvent {
    Opened,
    Cancelled,
}

pub(crate) struct AppChooser {
    path: PathBuf,
    info: FileInfo,
    apps: Vec<AppEntry>,
    backend: Arc<dyn AppOpener>,
    search: Entity<InputState>,
    query: String,
    selected: Option<String>,
    show_all: bool,
    make_default: bool,
    busy: bool,
    finished: bool,
    cancelled: Arc<AtomicBool>,
    error: Option<String>,
    _subscription: Subscription,
}

impl EventEmitter<AppChooserEvent> for AppChooser {}

impl AppChooser {
    pub(crate) fn new(
        path: PathBuf,
        info: FileInfo,
        apps: Vec<AppEntry>,
        backend: Arc<dyn AppOpener>,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search applications…"));
        let subscription =
            cx.subscribe_in(
                &search,
                window,
                |this, input, event, window, cx| match event {
                    InputEvent::Change => {
                        this.query = input.read(cx).value().trim().to_lowercase();
                        this.clear_hidden_selection();
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => this.submit(window, cx),
                    _ => {}
                },
            );
        let show_all = !apps.iter().any(|app| app.recommended);
        let make_default =
            info.association.is_some() && (info.default_app.is_none() || error.is_some());
        Self {
            path,
            info,
            apps,
            backend,
            search,
            query: String::new(),
            selected: None,
            show_all,
            make_default,
            busy: false,
            finished: false,
            cancelled: Arc::new(AtomicBool::new(false)),
            error,
            _subscription: subscription,
        }
    }

    fn matches(&self, app: &AppEntry) -> bool {
        (self.show_all || app.recommended) && app.name.to_lowercase().contains(&self.query)
    }

    fn clear_hidden_selection(&mut self) {
        if self.selected.as_ref().is_some_and(|id| {
            !self
                .apps
                .iter()
                .any(|app| &app.id == id && self.matches(app))
        }) {
            self.selected = None;
        }
    }

    pub(crate) fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.cancelled.store(true, Ordering::Relaxed);
        cx.emit(AppChooserEvent::Cancelled);
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.finished {
            return;
        }
        let Some(app) = self
            .apps
            .iter()
            .find(|app| Some(&app.id) == self.selected.as_ref())
            .cloned()
        else {
            return;
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        let backend = self.backend.clone();
        let path = self.path.clone();
        let association = self
            .make_default
            .then(|| self.info.association.clone())
            .flatten();
        let cancelled = self.cancelled.clone();
        let task = cx.background_executor().spawn(async move {
            if cancelled.load(Ordering::Relaxed) {
                return Err("Opening was cancelled".to_string());
            }
            backend.launch(&app, &path)?;
            let warning = if cancelled.load(Ordering::Relaxed) {
                None
            } else {
                association.and_then(|association| backend.set_default(&app, &association).err())
            };
            Ok(warning)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.finished { return; }
                this.busy = false;
                match result {
                    Ok(warning) => {
                        this.finished = true;
                        cx.emit(AppChooserEvent::Opened);
                        window.close_dialog(cx);
                        if let Some(error) = warning {
                            window.push_notification(
                                gpui_kit::component::notification::Notification::error(
                                    format!("Opened the file, but could not save the default application: {error}")
                                ).autohide(true), cx,
                            );
                        }
                    }
                    Err(error) => { this.error = Some(error); cx.notify(); }
                }
            });
        }).detach();
    }
}

impl Render for AppChooser {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self
            .path
            .file_name()
            .unwrap_or(self.path.as_os_str())
            .to_string_lossy()
            .into_owned();
        let rows: Vec<_> = self
            .apps
            .iter()
            .enumerate()
            .filter(|(_, app)| self.matches(app))
            .map(|(index, app)| {
                let id = app.id.clone();
                div()
                    .id(("app-choice-row", index))
                    .debug_selector(move || format!("app-choice-{index}"))
                    .child(
                        Button::new(("app-choice", index))
                            .label(app.name.clone())
                            .ghost()
                            .w_full()
                            .justify_start()
                            .selected(self.selected.as_ref() == Some(&app.id))
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected = Some(id.clone());
                                cx.notify();
                            })),
                    )
            })
            .collect();
        let has_other = self.apps.iter().any(|app| app.recommended)
            && self.apps.iter().any(|app| !app.recommended);
        div()
            .id("app-chooser")
            .debug_selector(|| "app-chooser".into())
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(div().text_sm().child(format!("Open “{name}”")))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.info.description.clone()),
            )
            .when(self.info.default_app.is_none(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .child("No default application is set. Choose an application to continue."),
                )
            })
            .child(Input::new(&self.search).cleanable(true))
            .child(
                div()
                    .id("app-chooser-list")
                    .h(px(240.0))
                    .overflow_y_scrollbar()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .when(rows.is_empty(), |this| {
                        this.child(div().text_sm().child(if self.apps.is_empty() {
                            "No applications available. Install an application, then try again."
                        } else {
                            "No matching applications."
                        }))
                    })
                    .children(rows),
            )
            .when(has_other, |this| {
                this.child(
                    Checkbox::new("app-chooser-show-all")
                        .label("Show all applications")
                        .checked(self.show_all)
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.show_all = *checked;
                            this.clear_hidden_selection();
                            cx.notify();
                        })),
                )
            })
            .child(
                div()
                    .id("app-chooser-default-control")
                    .debug_selector(|| "app-chooser-default".into())
                    .child(
                        Checkbox::new("app-chooser-default")
                            .label(format!(
                                "Always use this application for {}",
                                self.info.description
                            ))
                            .checked(self.make_default)
                            .disabled(self.busy || self.info.association.is_none())
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.make_default = *checked;
                                cx.notify();
                            })),
                    ),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(cx.theme().danger).child(error))
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("app-chooser-cancel-control")
                            .debug_selector(|| "app-chooser-cancel".into())
                            .child(Button::new("app-chooser-cancel").label("Cancel").on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.cancel(cx);
                                    window.close_dialog(cx);
                                }),
                            )),
                    )
                    .child(
                        div()
                            .id("app-chooser-open-control")
                            .debug_selector(|| "app-chooser-open".into())
                            .child(
                                Button::new("app-chooser-open")
                                    .label(if self.busy { "Opening…" } else { "Open" })
                                    .primary()
                                    .disabled(self.selected.is_none() || self.busy)
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.submit(window, cx)),
                                    ),
                            ),
                    ),
            )
    }
}
