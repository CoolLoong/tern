//! Browser commands and operation dialogs. Filesystem work belongs on workers.
use super::*;

impl BrowserView {
    pub(super) fn render_op_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let op = self.op.clone()?;
        let title = match op.kind {
            OpKind::NewFolder => "New Folder".to_string(),
            OpKind::NewTextFile => "New Empty Text File".to_string(),
            OpKind::Rename => "Rename".to_string(),
        };
        let viewport = window.viewport_size();
        let panel_w = px(420.0).min(viewport.width.max(px(320.0)));
        let x = (viewport.width - panel_w).max(px(0.0)) / 2.0;
        let y = px(140.0);

        let (popover, border, overlay, muted, accent) = {
            let t = cx.theme();
            (t.popover, t.border, t.overlay, t.muted, t.accent)
        };

        Some(
            div()
                .id("browser-op-backdrop")
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .size_full()
                .bg(overlay.opacity(0.35))
                .child(
                    div()
                        .id("browser-op")
                        .absolute()
                        .left(x)
                        .top(y)
                        .w(panel_w)
                        .bg(popover.opacity(0.98))
                        .border_1()
                        .border_color(border.opacity(0.9))
                        .rounded_md()
                        .shadow_lg()
                        .p(px(12.0))
                        .child(div().text_sm().child(title))
                        .child(
                            div()
                                .mt(px(10.0))
                                .child(Input::new(&op.input).cleanable(true)),
                        )
                        .child(
                            div()
                                .mt(px(12.0))
                                .flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .rounded_md()
                                        .px(px(10.0))
                                        .py(px(6.0))
                                        .bg(muted.opacity(0.2))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, _, _, cx| {
                                                this.op = None;
                                                cx.notify();
                                            }),
                                        )
                                        .child(div().text_xs().child("Cancel")),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .rounded_md()
                                        .px(px(10.0))
                                        .py(px(6.0))
                                        .bg(accent.opacity(0.3))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, _, window, cx| {
                                                this.confirm_op(window, cx);
                                            }),
                                        )
                                        .child(div().text_xs().child("OK")),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn confirm_op(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(op) = self.op.take() else {
            return;
        };
        let mut name = op.input.read(cx).value().trim().to_string();
        if matches!(op.kind, OpKind::NewTextFile) && name.is_empty() {
            name = "untitled.txt".into();
        }
        let cwd = self.current_dir(cx).unwrap_or_default();
        let target = self
            .table
            .read(cx)
            .delegate()
            .context_entry()
            .map(str::to_owned);
        let operation_dir = cwd.clone();
        let worker = cx.background_executor().spawn(async move {
            crate::files::operations::valid_name(&name)?;
            match op.kind {
                OpKind::NewFolder => {
                    crate::files::operations::create_dir(&operation_dir, &name).map(|_| ())
                }
                OpKind::NewTextFile => {
                    crate::files::operations::create_file(&operation_dir, &name).map(|_| ())
                }
                OpKind::Rename => {
                    let target = target.ok_or_else(|| std::io::Error::other("no file selected"))?;
                    let parent = crate::files::fs::parent_dir(&target)
                        .ok_or_else(|| std::io::Error::other("file has no parent directory"))?;
                    let destination = crate::files::fs::join(&parent, &name);
                    crate::files::operations::rename_path(&target, &destination)
                }
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = worker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Err(error) = result {
                    let note = gpui_kit::component::notification::Notification::error(format!(
                        "File operation failed: {error}"
                    ));
                    window.push_notification(note, cx);
                }
                // Refresh only after completion, without redirecting a tab
                // that navigated elsewhere while the operation was running.
                if this.current_dir(cx).as_deref() == Some(cwd.as_str()) {
                    this.table
                        .update(cx, |table, cx| table.delegate_mut().refresh_at(None, cx));
                }
                cx.notify();
            });
        })
        .detach();
        window.focus(&self.focus_handle(cx), cx);
        cx.notify();
    }

    pub(super) fn on_new_folder(
        &mut self,
        _: &NewFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.op = Some(Op {
            kind: OpKind::NewFolder,
            input: new_input(window, cx, "Folder name…"),
        });
        cx.notify();
    }

    pub(super) fn on_new_text_file(
        &mut self,
        _: &NewTextFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.op = Some(Op {
            kind: OpKind::NewTextFile,
            input: new_input(window, cx, "File name…"),
        });
        cx.notify();
    }

    pub(super) fn on_rename(&mut self, _: &Rename, window: &mut Window, cx: &mut Context<Self>) {
        self.op = Some(Op {
            kind: OpKind::Rename,
            input: new_input(window, cx, "New name…"),
        });
        cx.notify();
    }

    pub(super) fn on_move_to_trash(
        &mut self,
        _: &MoveToTrash,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table.clone();
        if crate::browser::targets::target_paths(&table, cx).is_empty() {
            return;
        }
        window.open_alert_dialog(cx, move |dialog, _window, cx| {
            let table = table.clone();
            let count = crate::browser::targets::target_paths(&table, cx).len();
            let name = crate::browser::targets::selected_name(&table, cx);
            let title = if count <= 1 {
                match name.as_deref() {
                    Some(n) => format!("Move “{n}” to Trash?"),
                    None => "Move selected item to Trash?".to_string(),
                }
            } else {
                format!("Move {count} items to Trash?")
            };
            dialog
                .title(title)
                .confirm()
                .button_props(
                    gpui_kit::component::dialog::DialogButtonProps::default()
                        .ok_text(gpui_kit::SharedString::from("Move to Trash"))
                        .ok_variant(gpui_kit::component::button::ButtonVariant::Danger)
                        .cancel_text(gpui_kit::SharedString::from("Cancel"))
                        .show_cancel(true),
                )
                .on_ok(move |_e, _window, cx| {
                    let paths = crate::browser::targets::target_paths(&table, cx);
                    if !paths.is_empty() {
                        crate::browser::transfers::move_to_trash(
                            paths,
                            crate::browser::transfers::next_group_id(),
                            cx,
                        );
                    }
                    true
                })
        });
    }

    pub(super) fn on_open_in_terminal(
        &mut self,
        _: &OpenInTerminal,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table.clone();
        let Some(dir) = crate::browser::targets::target_dir(&table, self.current_dir(cx), cx)
        else {
            return;
        };
        open_terminal_in(&dir);
    }

    pub(super) fn on_open_file(
        &mut self,
        _: &OpenFile,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chooser {
            cx.emit(BrowserEvent::AcceptSelection(self.selected_paths(cx)));
            return;
        }
        let table = self.table.clone();
        let paths = crate::browser::targets::target_paths(&table, cx);
        if paths.len() == 1 && paths[0].is_dir() {
            let dir = paths[0].to_string_lossy().into_owned();
            self.push_history(&dir, cx);
            self.change_dir(dir, cx);
            self.close_preview(cx);
        } else {
            crate::browser::targets::open_files(&paths);
            for path in &paths {
                crate::files::recent::record_opened(path, "tern", "xdg-open");
            }
        }
    }

    pub(super) fn on_open_with(
        &mut self,
        _: &OpenWith,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table.clone();
        let ids = table.read(cx).delegate().target_ids();
        let Some(path) = ids.first().cloned() else {
            return;
        };
        let mime = crate::platform::open_with::mime_for_path(&path);
        let apps = crate::platform::open_with::scan_desktop_apps();
        let matched = crate::platform::open_with::apps_for_mime(&mime, &apps);
        if matched.is_empty() {
            let note = gpui_kit::component::notification::Notification::info(format!(
                "No applications found for {mime}"
            ))
            .autohide(true);
            window.push_notification(note, cx);
            return;
        }
        match crate::platform::open_with::launch(&matched[0], &path) {
            Ok(()) => crate::files::recent::record_opened(
                std::path::Path::new(&path),
                &matched[0].name,
                &matched[0].exec,
            ),
            Err(err) => {
                let note = gpui_kit::component::notification::Notification::error(format!(
                    "Failed to open {}: {err}",
                    matched[0].name
                ))
                .autohide(true);
                window.push_notification(note, cx);
            }
        }
    }

    pub(super) fn on_restore_from_trash(
        &mut self,
        _: &RestoreFromTrash,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table.clone();
        let paths = crate::browser::targets::target_paths(&table, cx);
        let mut failed = 0usize;
        for path in &paths {
            if let Err(err) = crate::files::operations::restore_from_trash(path) {
                log::warn!("failed to restore {}: {err:#}", path.display());
                failed += 1;
            }
        }
        if failed > 0 {
            let state = cx.global_mut::<TransferCenterState>();
            state.note_failure(gpui_transfer::TransferFailure {
                label: "Restore".to_string(),
                count: failed,
            });
            state.note_completed();
        }
        if !paths.is_empty() {
            table.update(cx, |state, cx| state.delegate_mut().refresh_at(None, cx));
        }
    }

    pub(super) fn on_delete_permanently(
        &mut self,
        _: &Delete,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table.clone();
        let paths = crate::browser::targets::target_paths(&table, cx);
        for path in &paths {
            if let Err(err) = crate::files::operations::delete_permanently(path) {
                log::warn!("failed to delete {}: {err:#}", path.display());
            }
        }
        if !paths.is_empty() {
            table.update(cx, |state, cx| state.delegate_mut().refresh_at(None, cx));
        }
    }

    pub(super) fn on_add_to_bookmark(
        &mut self,
        _: &AddToBookmark,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table.clone();
        let ids = table.read(cx).delegate().target_ids();
        let Some(path) = ids.first().cloned() else {
            return;
        };
        let bookmarks = cx.global_mut::<crate::navigation::bookmarks::Bookmarks>();
        bookmarks.add(path);
        if let Err(err) = crate::navigation::bookmarks::save_to_disk(bookmarks) {
            log::warn!("failed to save bookmark: {err:#}");
        }
        cx.refresh_windows();
    }

    pub(super) fn on_properties(
        &mut self,
        _: &Properties,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table.clone();
        let ids = table.read(cx).delegate().target_ids();
        crate::browser::properties::show_properties(ids, window, cx);
    }

    pub(super) fn on_copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        let table = self.table.clone();
        let paths = crate::browser::targets::target_paths(&table, cx);
        let state = cx.global_mut::<crate::browser::clipboard::ClipboardState>();
        state.set(paths, false);
    }

    pub(super) fn on_cut(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        let table = self.table.clone();
        let paths = crate::browser::targets::target_paths(&table, cx);
        let state = cx.global_mut::<crate::browser::clipboard::ClipboardState>();
        state.set(paths, true);
    }

    pub(super) fn paste_to(&mut self, dst: String, cx: &mut Context<Self>) {
        let clipboard = cx.global_mut::<crate::browser::clipboard::ClipboardState>();
        if clipboard.is_empty() {
            return;
        }
        let (items, cut) = clipboard.take();
        let (label, op) = if cut {
            (
                "Moving…",
                crate::files::operations::move_path
                    as fn(&Path, &Path, &AtomicBool) -> std::io::Result<()>,
            )
        } else {
            (
                "Copying…",
                crate::files::operations::copy_path
                    as fn(&Path, &Path, &AtomicBool) -> std::io::Result<()>,
            )
        };
        crate::browser::transfers::start_transfer(
            items,
            dst,
            crate::browser::transfers::next_group_id(),
            label,
            op,
            cx,
        );
    }

    pub(super) fn on_paste(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dst) = self.current_dir(cx) else {
            return;
        };
        self.paste_to(dst, cx);
    }
}
