use super::*;
use crate::browser::app_chooser::{AppChooser, AppChooserEvent};
use crate::platform::open_with::{AppEntry, FileInfo};
use std::path::PathBuf;

struct PickerRequest {
    info: FileInfo,
    apps: Vec<AppEntry>,
    error: Option<String>,
}

impl BrowserView {
    pub(super) fn open_paths(
        &mut self,
        paths: Vec<PathBuf>,
        choose_app: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for path in paths {
            if self.opening_path.as_ref() != Some(&path)
                && !self
                    .open_requests
                    .iter()
                    .any(|(pending, _)| pending == &path)
            {
                self.open_requests.push_back((path, choose_app));
            }
        }
        self.open_next(window, cx);
    }

    fn open_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.opening_path.is_some() {
            return;
        }
        let Some((path, choose_app)) = self.open_requests.pop_front() else {
            return;
        };
        self.opening_path = Some(path.clone());
        let backend = self.app_opener.clone();
        let worker_path = path.clone();
        let task = cx.background_executor().spawn(async move {
            let info = backend.describe(&worker_path)?;
            let mut error = None;
            if !choose_app && let Some(app) = &info.default_app {
                match backend.launch(app, &worker_path) {
                    Ok(()) => return Ok(None),
                    Err(message) => {
                        error = Some(format!("Could not open with {}: {message}", app.name))
                    }
                }
            }
            let apps = backend.applications(&info)?;
            Ok::<_, String>(Some(PickerRequest { info, apps, error }))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Some(request)) => this.show_app_chooser(path, request, window, cx),
                result => {
                    this.opening_path = None;
                    if let Err(error) = result {
                        let name = path
                            .file_name()
                            .unwrap_or(path.as_os_str())
                            .to_string_lossy();
                        window.push_notification(
                            gpui_kit::component::notification::Notification::error(format!(
                                "Could not open {name}: {error}"
                            ))
                            .autohide(true),
                            cx,
                        );
                    }
                    this.open_next(window, cx);
                }
            });
        })
        .detach();
    }

    fn show_app_chooser(
        &mut self,
        path: PathBuf,
        request: PickerRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let backend = self.app_opener.clone();
        let chooser = cx.new(|cx| {
            AppChooser::new(
                path,
                request.info,
                request.apps,
                backend,
                request.error,
                window,
                cx,
            )
        });
        self.app_chooser_subscription =
            Some(
                cx.subscribe_in(&chooser, window, |this, _, event, window, cx| {
                    this.app_chooser = None;
                    this.app_chooser_subscription = None;
                    this.opening_path = None;
                    if matches!(event, AppChooserEvent::Cancelled) {
                        this.open_requests.clear();
                    }
                    this.open_next(window, cx);
                }),
            );
        self.app_chooser = Some(chooser.clone());
        window.open_dialog(cx, move |dialog, _, _| {
            let closed = chooser.clone();
            dialog
                .title("Open with…")
                .width(px(520.0))
                .child(chooser.clone())
                .on_close(move |_, _, cx| closed.update(cx, |this, cx| this.cancel(cx)))
        });
    }
}
