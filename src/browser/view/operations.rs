//! Browser commands and operation dialogs. Filesystem work belongs on workers.
use super::*;

impl BrowserView {
    pub(super) fn render_op_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let op = self.op.as_ref()?;
        let input = op.input.clone();
        let title = match op.kind {
            OpKind::NewFolder => "New Folder".to_string(),
            OpKind::NewTextFile => "New Empty Text File".to_string(),
            OpKind::Rename { .. } => "Rename".to_string(),
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
                .occlude()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .size_full()
                .bg(overlay.opacity(0.35))
                .on_action(cx.listener(
                    |this, _: &gpui_kit::component::input::Enter, window, cx| {
                        this.confirm_op(window, cx);
                    },
                ))
                .on_action(cx.listener(
                    |this, _: &gpui_kit::component::input::Escape, window, cx| {
                        this.cancel_op(window, cx);
                    },
                ))
                .child(
                    div()
                        .id("browser-op")
                        .debug_selector(|| "browser-op".into())
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
                        .child(div().mt(px(10.0)).child(Input::new(&input).cleanable(true)))
                        .child(
                            div()
                                .mt(px(12.0))
                                .flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .id("browser-op-cancel")
                                        .debug_selector(|| "browser-op-cancel".into())
                                        .cursor_pointer()
                                        .rounded_md()
                                        .px(px(10.0))
                                        .py(px(6.0))
                                        .bg(muted.opacity(0.2))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, _, window, cx| {
                                                this.cancel_op(window, cx);
                                            }),
                                        )
                                        .child(div().text_xs().child("Cancel")),
                                )
                                .child(
                                    div()
                                        .id("browser-op-confirm")
                                        .debug_selector(|| "browser-op-confirm".into())
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
        let mut name = op.input.read(cx).value().to_string();
        if !matches!(op.kind, OpKind::Rename { .. }) {
            name = name.trim().to_string();
        }
        if matches!(op.kind, OpKind::NewTextFile) && name.is_empty() {
            name = "untitled.txt".into();
        }
        let cwd = self.current_dir(cx).unwrap_or_default();
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
                OpKind::Rename { target } => {
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

    fn begin_op(
        &mut self,
        kind: OpKind,
        input: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        input.update(cx, |input, cx| input.focus(window, cx));
        self.op = Some(Op { kind, input });
        cx.notify();
    }

    fn cancel_op(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.op = None;
        window.focus(&self.focus_handle(cx), cx);
        cx.notify();
    }

    pub(super) fn on_new_folder(
        &mut self,
        _: &NewFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = new_input(window, cx, "Folder name…");
        self.begin_op(OpKind::NewFolder, input, window, cx);
    }

    pub(super) fn on_new_text_file(
        &mut self,
        _: &NewTextFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = new_input(window, cx, "File name…");
        self.begin_op(OpKind::NewTextFile, input, window, cx);
    }

    pub(super) fn on_rename(&mut self, _: &Rename, window: &mut Window, cx: &mut Context<Self>) {
        if self.op.is_some() {
            return;
        }
        let table = self.table.read(cx);
        let delegate = table.delegate();
        if delegate.in_trash() || delegate.in_recent() {
            return;
        }
        let targets = delegate.target_ids();
        let [target] = targets.as_slice() else {
            return;
        };
        let Some(row) = delegate.visible.iter().find(|row| row.id == *target) else {
            return;
        };
        let selected_len = if row.kind == EntryKind::Dir {
            row.name.len()
        } else {
            Path::new(&row.name)
                .file_stem()
                .map_or(row.name.len(), |stem| stem.len())
        };
        let name = row.name.clone();
        let input = new_input(window, cx, "File name…");
        input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
            input.set_selected_range(0..selected_len, cx);
        });
        self.begin_op(
            OpKind::Rename {
                target: target.clone(),
            },
            input,
            window,
            cx,
        );
    }

    pub(super) fn on_move_to_trash(
        &mut self,
        _: &MoveToTrash,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table.clone();
        let paths = crate::browser::targets::target_paths(&table, cx);
        let count = paths.len();
        if count == 0 {
            return;
        }
        let title = if count == 1 {
            match crate::browser::targets::selected_name(&table, cx).as_deref() {
                Some(name) => format!("Move “{name}” to Trash?"),
                None => "Move selected item to Trash?".to_string(),
            }
        } else {
            format!("Move {count} items to Trash?")
        };
        let paths: std::sync::Arc<[std::path::PathBuf]> = paths.into();
        window.open_alert_dialog(cx, move |dialog, _window, _cx| {
            let paths = paths.clone();
            dialog
                .title(title.clone())
                .confirm()
                .button_props(
                    gpui_kit::component::dialog::DialogButtonProps::default()
                        .ok_text(gpui_kit::SharedString::from("Move to Trash"))
                        .ok_variant(gpui_kit::component::button::ButtonVariant::Danger)
                        .cancel_text(gpui_kit::SharedString::from("Cancel"))
                        .show_cancel(true),
                )
                .on_ok(move |_e, _window, cx| {
                    crate::browser::transfers::move_to_trash(paths.clone(), cx);
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chooser {
            cx.emit(BrowserEvent::AcceptSelection(self.selected_paths(cx)));
            return;
        }
        let table = self.table.clone();
        let paths = crate::browser::targets::target_paths(&table, cx);
        let directory =
            paths.len() == 1
                && table.read(cx).delegate().visible.iter().any(|row| {
                    row.kind == EntryKind::Dir && std::path::Path::new(&row.id) == paths[0]
                });
        if directory {
            let dir = paths[0].to_string_lossy().into_owned();
            self.push_history(&dir, cx);
            self.change_dir(dir, cx);
            self.close_preview(cx);
        } else {
            self.open_paths(paths, false, window, cx);
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
        self.open_paths(vec![path.into()], true, window, cx);
    }

    pub(super) fn on_restore_from_trash(
        &mut self,
        _: &RestoreFromTrash,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table.clone();
        let paths = crate::browser::targets::target_paths(&table, cx);
        crate::browser::transfers::start_path_operation(
            paths,
            "Restore",
            crate::files::operations::restore_from_trash,
            cx,
        );
    }

    pub(super) fn on_delete_permanently(
        &mut self,
        _: &Delete,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table.clone();
        let paths = crate::browser::targets::target_paths(&table, cx);
        crate::browser::transfers::start_path_operation(
            paths,
            "Delete",
            crate::files::operations::delete_permanently,
            cx,
        );
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

    pub(super) fn copy_preview_text(&self, cx: &mut Context<Self>) {
        if let PreviewContent::Text { text, .. } = &self.preview.content {
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text.to_string()));
        }
    }

    pub(super) fn copy_preview_selection(&self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.preview.content, PreviewContent::Text { .. }) {
            let text = gpui_kit::base::TextSelection::selected_text(window, cx);
            if !text.is_empty() {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text));
            }
        }
        cx.stop_propagation();
    }

    pub(super) fn on_copy_path(
        &mut self,
        _: &CopyPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = crate::browser::targets::target_paths(&self.table, cx);
        let paths: std::io::Result<Vec<_>> = paths
            .into_iter()
            .filter(|path| !crate::files::recent::is_recent(&path.to_string_lossy()))
            .map(|path| std::path::absolute(path).map(|path| path.to_string_lossy().into_owned()))
            .collect();
        match paths {
            Ok(paths) if !paths.is_empty() => {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(paths.join("\n")));
            }
            Ok(_) => {}
            Err(error) => window.push_notification(
                gpui_kit::component::notification::Notification::error(format!(
                    "Could not copy path: {error}"
                )),
                cx,
            ),
        }
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
