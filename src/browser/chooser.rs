//! Dedicated portal dialog; it never opens a Tern workspace or persists tabs.
use super::view::{BrowserEvent, BrowserView};
use crate::platform::file_chooser::{APP_ID, ChooserRequest, Mode, Options, Pending, Resolution};
use gpui_kit::component::{
    ActiveTheme, Disableable, Root, TitleBar, WindowExt,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Subscription, Window, WindowBounds, WindowDecorations, WindowKind, WindowOptions, div,
    px, size,
};
use std::{path::PathBuf, sync::Arc};

pub(crate) fn run() -> anyhow::Result<()> {
    let (connection, requests) = crate::platform::file_chooser::connect()?;
    let settings = crate::settings::load_settings_from_disk().unwrap_or_default();
    gpui_kit::application()
        .with_quit_mode(gpui_kit::QuitMode::Explicit)
        .with_assets(crate::app::assets::TernAssets)
        .run(move |cx| {
            crate::app::runtime::init_app(cx, &settings);
            cx.spawn(async move |cx| {
                let _connection = connection;
                while let Ok(request) = requests.recv().await {
                    cx.update(|cx| open(request, cx));
                }
            })
            .detach();
        });
    Ok(())
}

fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        app_id: Some(APP_ID.into()),
        kind: WindowKind::Floating,
        window_bounds: Some(WindowBounds::centered(size(px(960.0), px(680.0)), cx)),
        window_min_size: Some(size(px(640.0), px(420.0))),
        titlebar: Some(TitleBar::title_bar_options()),
        window_decorations: Some(WindowDecorations::Client),
        ..Default::default()
    }
}
fn open(request: ChooserRequest, cx: &mut App) {
    if request.pending.completed() {
        return;
    }
    let pending = request.pending.clone();
    let cancelled = request.cancelled.clone();
    match cx.open_window(window_options(cx), move |window, cx| {
        window.set_window_title(&request.title);
        let dialog = cx.new(|cx| ChooserDialog::new(request, window, cx));
        cx.new(|cx| Root::new(dialog, window, cx))
    }) {
        Ok(handle) => {
            cx.spawn(async move |cx| {
                if cancelled.recv().await.is_ok() {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                }
            })
            .detach();
        }
        Err(error) => {
            log::error!("cannot open file chooser: {error:#}");
            pending.finish((2, Default::default()));
        }
    }
}

struct ChooserDialog {
    title: String,
    options: Options,
    pending: Arc<Pending>,
    browser: Entity<BrowserView>,
    filename: Entity<InputState>,
    message: String,
    overwrite: Option<PathBuf>,
    submitting: bool,
    _subscriptions: Vec<Subscription>,
}
impl Drop for ChooserDialog {
    fn drop(&mut self) {
        self.pending.cancel();
    }
}
impl ChooserDialog {
    fn new(request: ChooserRequest, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let browser = cx.new(|cx| {
            BrowserView::new_chooser(
                request.options.folder.to_string_lossy().into_owned(),
                window,
                cx,
            )
        });
        browser.update(cx, |browser, cx| {
            browser.set_file_filter(request.options.filter(), cx)
        });
        let filename =
            cx.new(|cx| InputState::new(window, cx).default_value(request.options.name.clone()));
        let subscription = cx.subscribe_in(&browser, window, |this, _, event, window, cx| {
            let BrowserEvent::AcceptSelection(paths) = event;
            if this.options.mode == Mode::Save
                && let Some(name) = paths.first().and_then(|path| path.file_name())
            {
                this.filename.update(cx, |input, cx| {
                    input.set_value(name.to_string_lossy().into_owned(), window, cx)
                });
            }
            this.submit(Some(paths.clone()), window, cx);
        });
        let filename_subscription =
            cx.subscribe_in(&filename, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit(None, window, cx);
                }
                if matches!(event, InputEvent::Change) {
                    this.overwrite = None;
                    this.message.clear();
                    cx.notify();
                }
            });
        Self {
            title: request.title,
            options: request.options,
            pending: request.pending,
            browser,
            filename,
            message: String::new(),
            overwrite: None,
            submitting: false,
            _subscriptions: vec![subscription, filename_subscription],
        }
    }
    fn submit(
        &mut self,
        selected: Option<Vec<PathBuf>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.submitting || self.pending.completed() {
            return;
        }
        let cwd = self.browser.read(cx).current_dir(cx).unwrap_or_default();
        let selected = selected.unwrap_or_else(|| self.browser.read(cx).selected_paths(cx));
        let name = self.filename.read(cx).value().to_string();
        let overwrite = self.overwrite.as_ref() == Some(&PathBuf::from(&cwd).join(&name));
        let options = self.options.clone();
        self.submitting = true;
        self.message.clear();
        cx.notify();
        let handle = window.window_handle();
        let task = cx.background_executor().spawn(async move {
            options
                .resolve(&cwd, &selected, &name, overwrite)
                .map(|result| (options, result))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let close = this
                .update(cx, |this, cx| {
                    this.submitting = false;
                    let close = match result {
                        Ok((options, Resolution::Selected(uris))) => {
                            this.pending.finish(options.response(uris))
                        }
                        Ok((_, Resolution::Overwrite(path))) => {
                            this.message = "文件已存在，再次点击替换以确认覆盖".into();
                            this.overwrite = Some(path);
                            false
                        }
                        Err(error) => {
                            this.message = error.to_string();
                            false
                        }
                    };
                    cx.notify();
                    close
                })
                .unwrap_or(false);
            if close {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
        })
        .detach();
    }
    fn cancel(&mut self, window: &mut Window) {
        self.pending.cancel();
        window.remove_window();
    }
}
impl Render for ChooserDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.clone();
        let mut controls = div().flex().flex_wrap().items_center().gap_2();
        if !self.options.filters.is_empty() {
            let index = self.options.filter_index;
            let filters = self.options.filters.clone();
            let dialog = cx.entity().downgrade();
            controls = controls.child(
                Button::new("chooser-filter")
                    .label(format!("类型：{}", self.options.filters[index].name))
                    .disabled(self.submitting)
                    .dropdown_menu(move |mut menu, _, _| {
                        for (selected, filter) in filters.iter().enumerate() {
                            let dialog = dialog.clone();
                            menu = menu.item(
                                PopupMenuItem::new(filter.name.clone())
                                    .checked(selected == index)
                                    .on_click(move |_, _, cx| {
                                        let _ = dialog.update(cx, |this, cx| {
                                            this.options.filter_index = selected;
                                            let filter = this.options.filter();
                                            this.browser.update(cx, |browser, cx| {
                                                browser.set_file_filter(filter, cx)
                                            });
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                        menu
                    }),
            );
        }
        for index in 0..self.options.choices.len() {
            let choice = self.options.choices[index].clone();
            let label = choice
                .options
                .iter()
                .find(|(id, _)| *id == choice.selected)
                .map(|(_, label)| label.as_str())
                .unwrap_or("");
            let dialog = cx.entity().downgrade();
            controls = controls.child(
                Button::new(("chooser-choice", index))
                    .label(format!("{}: {}", choice.label, label))
                    .disabled(self.submitting)
                    .dropdown_menu(move |mut menu, _, _| {
                        for (id, label) in &choice.options {
                            let id = id.clone();
                            let dialog = dialog.clone();
                            menu = menu.item(
                                PopupMenuItem::new(label.clone())
                                    .checked(id == choice.selected)
                                    .on_click(move |_, _, cx| {
                                        let _ = dialog.update(cx, |this, cx| {
                                            this.options.choices[index].selected = id.clone();
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                        menu
                    }),
            );
        }
        let save_name = (self.options.mode == Mode::Save).then(|| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .child("文件名")
                .child(Input::new(&self.filename))
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .on_key_down(
                cx.listener(|this, event: &gpui_kit::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" && !window.has_active_dialog(cx) {
                        this.cancel(window);
                        cx.stop_propagation();
                    }
                }),
            )
            .child(TitleBar::new().child(self.title.clone()))
            .child(
                div()
                    .px_2()
                    .py_1()
                    .child(browser.read(cx).nav_controls(&browser, cx)),
            )
            .child(div().flex_1().min_h_0().child(browser))
            .child(
                div()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(save_name)
                    .child(controls)
                    .child(div().text_sm().child(self.message.clone()))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("chooser-cancel").label("取消").on_click(
                                    cx.listener(|this, _, window, _| this.cancel(window)),
                                ),
                            )
                            .child(
                                Button::new("chooser-accept")
                                    .primary()
                                    .disabled(self.submitting)
                                    .label(if self.overwrite.is_some() {
                                        "替换".to_owned()
                                    } else {
                                        self.options.accept_label.clone()
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.submit(None, window, cx)
                                    })),
                            ),
                    ),
            )
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

#[cfg(test)]
mod chooser_tests {
    use super::*;
    use crate::platform::file_chooser::{Dictionary, mock_request, owned};
    #[gpui_kit::test]
    fn portal_dialog_is_floating_and_close_cancels_without_native_windows(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            crate::app::runtime::init_app(cx, &crate::settings::SettingsFile::default());
            let options = window_options(cx);
            assert_eq!(options.kind, WindowKind::Floating);
            assert_eq!(options.app_id.as_deref(), Some(APP_ID));
        });
        let mut values = Dictionary::new();
        values.insert(
            "current_folder".into(),
            owned(std::env::temp_dir().to_string_lossy().as_bytes().to_vec()),
        );
        let (request, response) = mock_request(Options::parse(Mode::Open, &values).unwrap());
        let pending = request.pending.clone();
        cx.update(|cx| open(request, cx));
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(cx.windows().len(), 1));
        pending.cancel();
        cx.run_until_parked();
        assert_eq!(response.try_recv().unwrap().0, 1);
        cx.update(|cx| assert!(cx.windows().is_empty()));
    }

    #[gpui_kit::test]
    fn portal_dialog_returns_selected_uri_and_closes(cx: &mut gpui_kit::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("tern-picker-ui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("选择.txt");
        std::fs::write(&file, b"test").unwrap();
        cx.update(|cx| {
            crate::app::runtime::init_app(cx, &crate::settings::SettingsFile::default())
        });
        let mut values = Dictionary::new();
        values.insert(
            "current_folder".into(),
            owned(dir.to_string_lossy().as_bytes().to_vec()),
        );
        let (request, response) = mock_request(Options::parse(Mode::Open, &values).unwrap());
        cx.update(|cx| open(request, cx));
        cx.run_until_parked();
        let handle = cx.update(|cx| cx.windows()[0].downcast::<Root>().unwrap());
        handle
            .update(cx, |root, window, cx| {
                let dialog = root.view().clone().downcast::<ChooserDialog>().unwrap();
                dialog.update(cx, |dialog, cx| {
                    dialog.submit(Some(vec![file.clone()]), window, cx)
                });
            })
            .unwrap();
        cx.run_until_parked();
        let (code, result) = response.try_recv().unwrap();
        assert_eq!(code, 0);
        let uris = Vec::<String>::try_from(result["uris"].try_clone().unwrap()).unwrap();
        assert_eq!(
            url::Url::parse(&uris[0]).unwrap().to_file_path().unwrap(),
            file
        );
        cx.update(|cx| assert!(cx.windows().is_empty()));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
