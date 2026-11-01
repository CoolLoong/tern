//! Browser state, navigation, input subscriptions and preview loading.
#[cfg(test)]
mod chooser_tests;
#[cfg(test)]
mod interaction_tests;
mod layout;
mod opening;
#[cfg(test)]
mod opening_tests;
mod operations;
#[cfg(test)]
mod preview_tests;
#[cfg(test)]
mod rename_tests;

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use gpui_kit::component::{
    ActiveTheme, Disableable, FocusableExt, Icon, IconName, WindowExt,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, ButtonCustomVariant, ButtonVariant, ButtonVariants},
    dialog::DialogButtonProps,
    input::{Input, InputEvent, InputState},
    menu::{ContextMenuExt, DropdownMenu, PopupMenuItem},
    scroll::ScrollableElement,
    table::{DataTable, TableEvent, TableState},
};
use gpui_kit::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, Render, Styled, StyledImage, Subscription, Window, div,
    img, prelude::FluentBuilder, px,
};

use crate::app::actions::*;
use crate::browser::context_menu::{build_context_menu, open_terminal_in};
use crate::browser::format::human_bytes;
use crate::browser::marquee::{Marquee, marquee_allows_start, marquee_apply, marquee_hit_row};
use crate::browser::preview::{
    PreviewContent, PreviewGate, PreviewKind, PreviewPane, PreviewTarget, fenced_text_as_markdown,
    gate_preview,
};
use crate::browser::table::{BrowserDelegate, BrowserStatus};
use crate::files::fs;
use crate::files::model::EntryKind;
use gpui_transfer::TransferCenterState;

type AnyElement = gpui_kit::AnyElement;

const HISTORY_LIMIT: usize = 100;

pub(crate) enum BrowserEvent {
    AcceptSelection(Vec<std::path::PathBuf>),
}
impl gpui_kit::EventEmitter<BrowserEvent> for BrowserView {}

pub struct BrowserView {
    chooser: bool,
    table: Entity<TableState<BrowserDelegate>>,
    path_input: Entity<InputState>,
    path_editing: bool,
    search_input: Entity<InputState>,
    search_open: bool,
    focus_handle: FocusHandle,
    back_stack: Vec<String>,
    forward_stack: Vec<String>,
    op: Option<Op>,
    show_preview: bool,
    preview: PreviewPane,
    preview_epoch: usize,
    marquee: Option<Marquee>,
    ops_epoch: u64,
    app_opener: std::sync::Arc<dyn crate::platform::open_with::AppOpener>,
    open_requests: std::collections::VecDeque<(std::path::PathBuf, bool)>,
    opening_path: Option<std::path::PathBuf>,
    app_chooser: Option<Entity<crate::browser::app_chooser::AppChooser>>,
    app_chooser_subscription: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

struct Op {
    kind: OpKind,
    input: Entity<InputState>,
}

#[derive(Debug)]
enum OpKind {
    NewFolder,
    NewTextFile,
    Rename { target: String },
}

impl Focusable for BrowserView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn new_input<T>(
    window: &mut Window,
    cx: &mut Context<T>,
    placeholder: impl Into<String>,
) -> Entity<InputState> {
    let placeholder = placeholder.into();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

fn table_event_activation_row(ev: &TableEvent) -> Option<usize> {
    match ev {
        TableEvent::DoubleClickedRow(row_ix) | TableEvent::DoubleClickedCell(row_ix, _) => {
            Some(*row_ix)
        }
        _ => None,
    }
}

impl BrowserView {
    pub(crate) fn new_at(
        initial_dir: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let table = cx.new(|cx| {
            TableState::new(BrowserDelegate::new(), window, cx)
                .sortable(false)
                .col_selectable(false)
                .col_movable(false)
                .row_selectable(false)
        });
        let path_input = new_input(window, cx, "Enter path…");
        let search_input = new_input(window, cx, "Search this folder…");

        table.update(cx, |state, cx| {
            if let Some(dir) = initial_dir {
                state.delegate_mut().cd(dir, cx);
            } else {
                state.delegate_mut().bootstrap_root(cx);
            }
        });

        let path_sub = Self::subscribe_path_input(&path_input, &table, window, cx);
        let search_sub = Self::subscribe_search_input(&search_input, &table, window, cx);
        let table_events = Self::subscribe_table_events(&table, window, cx);
        let table_observer = cx.observe(&table, |_, _, cx| cx.notify());
        let ops_observer =
            cx.observe_global_in::<TransferCenterState>(window, |this, window, cx| {
                this.refresh_after_ops(window, cx);
            });

        Self {
            chooser: false,
            table,
            path_input,
            path_editing: false,
            search_input,
            search_open: false,
            focus_handle,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            op: None,
            show_preview: false,
            preview: PreviewPane {
                target: None,
                content: PreviewContent::Empty,
            },
            preview_epoch: 0,
            marquee: None,
            ops_epoch: cx.global::<TransferCenterState>().completed_epoch(),
            app_opener: std::sync::Arc::new(crate::platform::open_with::SystemAppOpener),
            open_requests: Default::default(),
            opening_path: None,
            app_chooser: None,
            app_chooser_subscription: None,
            _subscriptions: vec![
                path_sub,
                search_sub,
                table_events,
                table_observer,
                ops_observer,
            ],
        }
    }

    pub fn status(&self, cx: &App) -> BrowserStatus {
        self.table.read(cx).delegate().status()
    }

    pub(crate) fn new_chooser(dir: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new_at(Some(dir), window, cx);
        view.chooser = true;
        view.table.update(cx, |table, cx| {
            table.delegate_mut().set_recursive_search(false, cx);
        });
        view
    }

    pub(crate) fn selected_paths(&self, cx: &App) -> Vec<std::path::PathBuf> {
        self.table
            .read(cx)
            .delegate()
            .selected_ids_sorted()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub(crate) fn set_file_filter(
        &mut self,
        filter: Option<crate::files::filter::FileFilter>,
        cx: &mut Context<Self>,
    ) {
        self.table.update(cx, |table, cx| {
            table.delegate_mut().set_file_filter(filter, cx)
        });
    }

    pub fn current_dir(&self, cx: &App) -> Option<String> {
        self.table.read(cx).delegate().directory.clone()
    }

    pub fn change_dir(&mut self, dir: String, cx: &mut Context<Self>) {
        log::debug!("navigate -> {dir}");
        // Leaving the folder drops its filter; the box is re-synced in
        // `on_search`, which is the only place a Window is available.
        self.search_open = false;
        let table = self.table.clone();
        table.update(cx, |state, cx| state.delegate_mut().cd(dir, cx));
        cx.notify();
    }

    pub(crate) fn select_paths(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        self.table.update(cx, |table, cx| {
            table.delegate_mut().select_after_load(paths);
            cx.notify();
        });
    }

    fn refresh_after_ops(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let epoch = cx.global::<TransferCenterState>().completed_epoch();
        if epoch == self.ops_epoch {
            return;
        }
        self.ops_epoch = epoch;

        // Report failures before refreshing: silently leaving a file in place
        // while the list redraws looks like success.
        let failure = cx.global::<TransferCenterState>().last_failure().cloned();
        if let Some(failure) = failure {
            cx.global_mut::<TransferCenterState>().clear_failure();
            let note = gpui_kit::component::notification::Notification::error(format!(
                "{} failed for {} item(s)",
                failure.label, failure.count
            ))
            .autohide(true);
            window.push_notification(note, cx);
        }

        let table = self.table.clone();
        table.update(cx, |state, cx| {
            let target = state.selected_row();
            state.delegate_mut().refresh_at(target, cx);
        });
    }

    pub(crate) fn refresh(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let table = self.table.clone();
        table.update(cx, |state, cx| {
            let target = state.selected_row();
            state.clear_selection(cx);
            state.delegate_mut().refresh_at(target, cx);
        });
    }

    pub(crate) fn toggle_hidden(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let table = self.table.clone();
        table.update(cx, |state, cx| {
            let d = state.delegate_mut();
            d.set_show_hidden(!d.show_hidden(), cx);
            cx.notify();
        });
    }

    fn subscribe_path_input(
        path_input: &Entity<InputState>,
        table: &Entity<TableState<BrowserDelegate>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        let _table = table.clone();
        cx.subscribe_in(
            path_input,
            window,
            move |this, input, ev, window, cx| match ev {
                InputEvent::PressEnter { .. } => {
                    let path = input.read(cx).value().to_string();
                    let path = path.trim();
                    if !path.is_empty() {
                        this.cd_to(path, cx);
                        this.path_editing = false;
                        window.focus(&this.focus_handle(cx), cx);
                        cx.notify();
                    }
                }
                InputEvent::Focus => {
                    this.path_editing = true;
                    cx.notify();
                }
                InputEvent::Blur => {
                    this.path_editing = false;
                    cx.notify();
                }
                _ => {}
            },
        )
    }

    fn subscribe_search_input(
        search_input: &Entity<InputState>,
        table: &Entity<TableState<BrowserDelegate>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        let table = table.clone();
        cx.subscribe_in(search_input, window, move |this, input, ev, window, cx| {
            match ev {
                InputEvent::Change => {
                    let query = input.read(cx).value().to_string();
                    table.update(cx, |state, cx| {
                        state.delegate_mut().set_query(query, cx);
                    });
                }
                InputEvent::PressEnter { .. } => {
                    this.search_open = false;
                    window.focus(&this.focus_handle(cx), cx);
                    cx.notify();
                }
                // Escape is handled by the `Cancel` action, which clears the query.
                InputEvent::Blur => {
                    this.search_open = false;
                    cx.notify();
                }
                InputEvent::Focus => {
                    this.search_open = true;
                    cx.notify();
                }
            }
        })
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = false;
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        let table = self.table.clone();
        table.update(cx, |state, cx| {
            state.delegate_mut().set_query(String::new(), cx);
        });
        window.focus(&self.focus_handle(cx), cx);
        cx.notify();
    }

    fn on_search(&mut self, _: &Search, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = true;
        // The delegate owns the query; the box merely displays it. Re-syncing on
        // open is what clears leftover text after navigating away.
        let query = self.table.read(cx).delegate().query().to_string();
        self.search_input
            .update(cx, |input, cx| input.set_value(query.clone(), window, cx));
        window.focus(&self.search_input.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    fn on_cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.path_editing {
            self.path_editing = false;
            window.focus(&self.focus_handle(cx), cx);
            cx.notify();
            return;
        }
        if self.search_open {
            self.close_search(window, cx);
        }
    }

    fn subscribe_table_events(
        table: &Entity<TableState<BrowserDelegate>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        let table_entity = table.clone();
        let table_for_handler = table_entity.clone();
        cx.subscribe_in(
            &table_entity,
            window,
            move |this, _table_state, ev, window, cx| {
                this.handle_table_event(&table_for_handler, ev, window, cx);
            },
        )
    }

    fn handle_table_event(
        &mut self,
        table: &Entity<TableState<BrowserDelegate>>,
        ev: &TableEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let TableEvent::SelectRow(row_ix) = ev {
            self.handle_row_selected(table, *row_ix, cx);
        }

        let activate_row = table_event_activation_row(ev);

        if let Some(row_ix) = activate_row
            && let Some(row) = table.read(cx).delegate().row(row_ix).cloned()
        {
            if row.kind == EntryKind::Dir && !crate::files::recent::is_recent(&row.id) {
                self.push_history(&row.id, cx);
                self.change_dir(row.id, cx);
                window.focus(&self.focus_handle(cx), cx);
                cx.stop_propagation();
            } else if self.chooser {
                cx.emit(BrowserEvent::AcceptSelection(vec![row.id.into()]));
                cx.stop_propagation();
            } else {
                self.open_paths(vec![row.id.into()], false, window, cx);
                cx.stop_propagation();
            }
        }
    }

    fn secondary_press(
        table: &Entity<TableState<BrowserDelegate>>,
        row: Option<usize>,
        cx: &mut App,
    ) {
        table.update(cx, |state, cx| {
            state.delegate_mut().secondary_press(row);
            state.clear_selection(cx);
            state.set_right_clicked_row(row, cx);
            cx.notify();
        });
    }

    /// Records the current location before moving to `target`.
    ///
    /// Mirrors nemo's `handle_go_elsewhere`: the forward list is clobbered, and
    /// re-entering the location we are already on records nothing, so the back
    /// button never appears to do nothing.
    fn push_history(&mut self, target: &str, cx: &mut Context<Self>) {
        let Some(current) = self.current_dir(cx) else {
            return;
        };
        if current == target {
            return;
        }
        self.back_stack.push(current);
        if self.back_stack.len() > HISTORY_LIMIT {
            self.back_stack.remove(0);
        }
        self.forward_stack.clear();
    }

    /// Nemo's `bookmark_list_get_uri_index`: when the target is already on the
    /// back list, going there means rewinding to that entry rather than
    /// appending a new one.
    fn history_index_of(&self, target: &str) -> Option<usize> {
        self.back_stack.iter().rposition(|dir| dir == target)
    }

    fn handle_row_selected(
        &mut self,
        table: &Entity<TableState<BrowserDelegate>>,
        row_ix: usize,
        cx: &mut Context<Self>,
    ) {
        let target = table
            .read(cx)
            .delegate()
            .row(row_ix)
            .map(|row| PreviewTarget {
                id: row.id.clone(),
                name: row.name.clone(),
                kind: row.kind,
                size: row.size,
            });

        let gate = target
            .as_ref()
            .map(|t| gate_preview(&t.name, t.kind, t.size))
            .unwrap_or(PreviewGate::Hidden);

        match gate {
            PreviewGate::Allowed { .. } => {
                self.show_preview = true;
                self.request_preview(target, cx);
            }
            PreviewGate::Hidden => self.close_preview(cx),
        }
    }

    fn close_preview(&mut self, cx: &mut Context<Self>) {
        if !self.show_preview && self.preview.target.is_none() {
            return;
        }
        self.show_preview = false;
        self.preview_epoch = self.preview_epoch.wrapping_add(1);
        self.preview = PreviewPane {
            target: None,
            content: PreviewContent::Empty,
        };
        cx.notify();
    }

    fn request_preview(&mut self, target: Option<PreviewTarget>, cx: &mut Context<Self>) {
        enum PreviewOutcome {
            Content(PreviewContent),
            Error(gpui_kit::SharedString),
        }

        fn load_outcome(target: &PreviewTarget, kind: PreviewKind, limit: usize) -> PreviewOutcome {
            let (bytes, truncated) = match crate::files::fs::read_bytes_limit(&target.id, limit) {
                Ok(res) => res,
                Err(err) => {
                    return PreviewOutcome::Error(format!("Read failed: {err}").into());
                }
            };
            if truncated {
                return PreviewOutcome::Content(PreviewContent::Binary);
            }
            match kind {
                PreviewKind::Image(format) => {
                    let image = std::sync::Arc::new(gpui_kit::Image::from_bytes(format, bytes));
                    PreviewOutcome::Content(PreviewContent::Image { image })
                }
                PreviewKind::Text => {
                    if bytes.contains(&0) {
                        return PreviewOutcome::Content(PreviewContent::Binary);
                    }
                    let s = String::from_utf8_lossy(&bytes).to_string();
                    let fenced = fenced_text_as_markdown(&target.name, &s);
                    PreviewOutcome::Content(PreviewContent::Text {
                        text: s.into(),
                        fenced_markdown: fenced.into(),
                    })
                }
            }
        }

        let Some(target) = target else {
            self.close_preview(cx);
            return;
        };
        let gate = gate_preview(&target.name, target.kind, target.size);
        let PreviewGate::Allowed { kind, limit_bytes } = gate else {
            self.close_preview(cx);
            return;
        };

        let epoch = self.preview_epoch.wrapping_add(1);
        self.preview_epoch = epoch;
        self.preview.target = Some(target.clone());
        self.preview.content = PreviewContent::Loading;
        cx.notify();

        let worker = cx.background_executor().spawn(async move {
            let outcome = load_outcome(&target, kind, limit_bytes);
            (target, outcome)
        });
        cx.spawn(async move |this, cx| {
            let (target, outcome) = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.preview_epoch != epoch {
                    return;
                }
                match outcome {
                    PreviewOutcome::Content(content) => {
                        if matches!(content, PreviewContent::Binary) {
                            this.close_preview(cx);
                            return;
                        }
                        this.preview = PreviewPane {
                            target: Some(target.clone()),
                            content,
                        };
                        cx.notify();
                    }
                    PreviewOutcome::Error(message) => {
                        this.preview = PreviewPane {
                            target: Some(target.clone()),
                            content: PreviewContent::Error { message },
                        };
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn cd_to(&mut self, path: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.history_index_of(path) {
            // Already visited: rewind to that entry instead of appending a new
            // one, moving everything we skip past onto the forward list.
            while self.back_stack.len() > index {
                let dir = self
                    .back_stack
                    .pop()
                    .expect("index is within the back stack");
                self.forward_stack.push(dir);
            }
            self.change_dir(path.to_string(), cx);
            return;
        }
        self.push_history(path, cx);
        self.change_dir(path.to_string(), cx);
    }

    fn navigate_back(&mut self, cx: &mut Context<Self>) {
        if let Some(dir) = self.back_stack.pop() {
            if let Some(cur) = self.current_dir(cx) {
                self.forward_stack.push(cur);
            }
            self.change_dir(dir, cx);
        }
    }

    fn navigate_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(dir) = self.forward_stack.pop() {
            if let Some(cur) = self.current_dir(cx) {
                self.back_stack.push(cur);
            }
            self.change_dir(dir, cx);
        }
    }

    fn navigate_up(&mut self, cx: &mut Context<Self>) {
        let Some(dir) = self.current_dir(cx) else {
            return;
        };
        if let Some(parent) = fs::parent_dir(&dir) {
            self.push_history(&parent, cx);
            self.change_dir(parent, cx);
        }
    }

    fn on_back(&mut self, _: &GoBack, _window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_back(cx);
    }

    fn on_forward(&mut self, _: &GoForward, _window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_forward(cx);
    }

    fn on_up(&mut self, _: &GoUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_up(cx);
    }

    fn on_refresh(&mut self, _: &Refresh, _window: &mut Window, cx: &mut Context<Self>) {
        let table = self.table.clone();
        table.update(cx, |state, cx| {
            let target = state.selected_row();
            state.clear_selection(cx);
            state.delegate_mut().refresh_at(target, cx);
        });
    }

    fn on_toggle_hidden(
        &mut self,
        _: &ToggleHiddenFiles,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let table = self.table.clone();
        table.update(cx, |state, cx| {
            let d = state.delegate_mut();
            d.set_show_hidden(!d.show_hidden(), cx);
            cx.notify();
        });
    }

    fn on_move_selection(&mut self, delta: i64, cx: &mut Context<Self>) {
        let table = self.table.clone();
        let moved = table.update(cx, |state, cx| {
            let row_ix = state.delegate_mut().move_selection(delta);
            cx.notify();
            row_ix
        });
        if let Some(row_ix) = moved {
            self.handle_row_selected(&table, row_ix, cx);
        }
    }

    fn on_select_up(&mut self, _: &SelectUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.on_move_selection(-1, cx);
    }

    fn on_select_down(&mut self, _: &SelectDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.on_move_selection(1, cx);
    }

    fn on_select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        let table = self.table.clone();
        table.update(cx, |state, cx| {
            state.delegate_mut().select_all();
            state.clear_selection(cx);
            cx.notify();
        });
    }

    fn on_toggle_path_entry(
        &mut self,
        _: &OpenPathEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.path_editing {
            self.path_editing = false;
            window.focus(&self.focus_handle(cx), cx);
            cx.notify();
            return;
        }
        self.begin_path_edit(window, cx);
    }

    fn begin_path_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.path_editing = true;
        self.search_open = false;
        let cwd = self.current_dir(cx).unwrap_or_default();
        self.path_input.update(cx, |input, cx| {
            input.set_value(cwd, window, cx);
        });
        window.focus(&self.path_input.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
}
