//! Listing snapshots, navigation, selection and cancellable jobs.
#[cfg(test)]
mod listing_tests;
mod render;

use std::collections::HashSet;

use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable,
    table::{Column, TableDelegate, TableEvent, TableState},
};
use gpui_kit::{
    App, AppContext, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, StatefulInteractiveElement, Styled, StyledImage,
    Window, div, img, prelude::FluentBuilder, px,
};

use crate::browser::drag_drop::LocalDragEntry;
use crate::browser::preview::{file_icon_path, is_json_name};
use crate::files::jobs::JobSlot;
use crate::files::listing::{Origin, RowData, RowSource, RowView, ViewOptions};
use crate::files::model::{EntryKind, SortColumn, SortDirection, SortSpec, VisibleRow};

pub(crate) fn file_icon(
    row: &VisibleRow,
    accent: gpui_kit::Hsla,
    muted: gpui_kit::Hsla,
) -> gpui_kit::AnyElement {
    use gpui_common::TermuaIcon;
    match row.kind {
        EntryKind::Dir => img(TermuaIcon::FolderClosedBlue)
            .w(px(16.0))
            .h(px(16.0))
            .object_fit(gpui_kit::ObjectFit::Contain)
            .into_any_element(),
        _ => {
            if is_json_name(&row.name) {
                Icon::default()
                    .path("icons/braces.svg")
                    .small()
                    .text_color(gpui_kit::rgb(0x2563eb))
                    .into_any_element()
            } else if let Some(icon) = file_icon_path(&row.name) {
                Icon::default()
                    .path(icon)
                    .small()
                    .text_color(accent.opacity(0.95))
                    .into_any_element()
            } else {
                Icon::new(IconName::File)
                    .text_color(muted)
                    .into_any_element()
            }
        }
    }
}

/// Root of the trash payload directory, memoized so `rebuild_visible` can compare
/// against it without touching the filesystem on every keystroke.
fn trash_files_root() -> &'static str {
    static ROOT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        crate::files::operations::trash_files_dir()
            .to_string_lossy()
            .into_owned()
    })
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
enum SelectionMode {
    #[default]
    Idle,
    DragSelect,
    Marquee,
}

#[derive(Default, PartialEq)]
enum ContextTarget {
    #[default]
    None,
    Background,
    Entry(String),
}

#[derive(Default)]
enum RowGesture {
    #[default]
    Idle,
    Extend,
    Deferred,
}

#[derive(Default)]
struct BrowserInteraction {
    selected_ids: HashSet<String>,
    selection_anchor_id: Option<String>,
    selection_mode: SelectionMode,
    context_target: ContextTarget,
    gesture: RowGesture,
    dragging: bool,
    press_on_row: bool,
}

pub(crate) enum BrowserMenuTarget {
    Background,
    Single(EntryKind),
    Multi { has_dir: bool },
}

pub struct BrowserDelegate {
    pub directory: Option<String>,
    pub(crate) loading: HashSet<String>,
    load_job: JobSlot,
    search_job: JobSlot,
    view_job: JobSlot,
    pending_selection: Option<Vec<String>>,
    searching: bool,
    pub show_hidden: bool,
    file_filter: Option<std::sync::Arc<crate::files::filter::FileFilter>>,
    interaction: BrowserInteraction,
    pub columns: Vec<Column>,
    pub sort: SortSpec,
    pub(crate) visible: RowView,
    // Immutable sources include hidden rows. Sorting and visibility changes
    // build only index arrays on workers, so clearing search does not reread IO.
    all_rows: RowSource,
    query: String,
    search_rows: Option<RowSource>,
    /// The last walk hit its result or depth cap, so the listing is a prefix.
    search_truncated: bool,
}

impl BrowserDelegate {
    pub fn new() -> Self {
        Self {
            directory: None,
            loading: HashSet::new(),
            load_job: JobSlot::default(),
            search_job: JobSlot::default(),
            view_job: JobSlot::default(),
            pending_selection: None,
            searching: false,
            show_hidden: false,
            file_filter: None,
            interaction: BrowserInteraction::default(),
            columns: browser_table_columns(false),
            sort: SortSpec::default(),
            visible: RowView::default(),
            all_rows: RowSource::default(),
            query: String::new(),
            search_rows: None,
            search_truncated: false,
        }
    }

    pub fn bootstrap_root(&mut self, cx: &mut Context<TableState<Self>>) {
        let root = home::home_dir()
            .or_else(|| std::env::current_dir().ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        self.cd(root, cx);
    }

    fn view_options(&self) -> ViewOptions {
        ViewOptions {
            sort: self.sort,
            show_hidden: self.show_hidden,
            filter: self.file_filter.clone(),
        }
    }

    pub(crate) fn set_file_filter(
        &mut self,
        filter: Option<crate::files::filter::FileFilter>,
        cx: &mut Context<TableState<Self>>,
    ) {
        self.file_filter = filter.map(std::sync::Arc::new);
        self.rebuild_visible(cx);
    }

    fn origin(&self) -> Origin {
        if self.in_recent() {
            Origin::Recent
        } else if self.in_trash() {
            Origin::Trash
        } else {
            Origin::Directory
        }
    }

    pub fn refresh_dir(&mut self, dir: String, cx: &mut Context<TableState<Self>>) {
        if self.directory.as_deref() != Some(&dir) {
            return;
        }
        let cancel = self.load_job.start();
        self.loading.clear();
        self.loading.insert(dir.clone());
        let options = self.view_options();
        let origin = self.origin();
        cx.notify();
        let worker_cancel = cancel.clone();
        let task = cx.background_executor().spawn(async move {
            crate::files::listing::load_directory(&dir, origin, options, &worker_cancel)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |table, cx| {
                let delegate = table.delegate_mut();
                if !delegate.load_job.finish(&cancel) {
                    retire(result, cx);
                    return;
                }
                delegate.loading.clear();
                match result {
                    Ok(view) => {
                        let old = std::mem::replace(&mut delegate.all_rows, view.source());
                        retire(old, cx);
                        if delegate.has_query() {
                            // A directory refresh cannot overwrite live search results.
                            retire(view, cx);
                        } else if view.options == delegate.view_options() {
                            delegate.view_job.cancel();
                            Self::publish_view(table, view, cx);
                        } else {
                            // Sort/hidden settings changed while IO was in flight.
                            retire(view, cx);
                            delegate.rebuild_visible(cx);
                        }
                    }
                    Err(error) => log::warn!("failed to read directory: {error:#}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn cd(&mut self, dir: String, cx: &mut Context<TableState<Self>>) {
        self.load_job.cancel();
        self.search_job.cancel();
        self.view_job.cancel();
        self.pending_selection = None;
        self.query.clear();
        self.searching = false;
        self.search_truncated = false;
        self.loading.clear();
        self.directory = Some(dir.clone());
        self.clear_selection();
        let old = (
            std::mem::take(&mut self.all_rows),
            self.search_rows.take(),
            std::mem::take(&mut self.visible),
        );
        retire(old, cx);
        self.update_columns();
        self.refresh_dir(dir, cx);
    }

    pub(crate) fn select_after_load(&mut self, paths: Vec<String>) {
        if paths.iter().any(|path| {
            std::path::Path::new(path)
                .file_name()
                .is_some_and(|name| name.as_encoded_bytes().starts_with(b"."))
        }) {
            self.show_hidden = true;
        }
        self.pending_selection = Some(paths);
    }

    pub fn refresh_at(&mut self, target: Option<usize>, cx: &mut Context<TableState<Self>>) {
        let Some(dir) = self.directory.clone() else {
            return;
        };
        // While a search is live the listing is the walk's results, and a plain
        // directory read only refreshes `all_rows` -- so entries deleted or
        // moved since the walk would stay on screen. Re-run the walk instead.
        if self.has_query() {
            self.restart_search(cx);
            return;
        }
        let selected = target.and_then(|ix| self.row(ix).map(|r| r.id.clone()));
        self.refresh_dir(dir, cx);
        if let Some(id) = selected
            && let Some(ix) = self.visible.iter().position(|r| r.id == id)
        {
            cx.notify();
            let _ = ix;
        }
    }

    pub fn secondary_press(&mut self, row_ix: Option<usize>) {
        let target = row_ix.and_then(|row_ix| self.row(row_ix).map(|row| row.id.clone()));
        match target {
            Some(target) => {
                if !self.interaction.selected_ids.contains(&target) {
                    self.interaction.selected_ids.clear();
                    self.interaction.selected_ids.insert(target.clone());
                    self.interaction.selection_anchor_id = Some(target.clone());
                }
                self.interaction.context_target = ContextTarget::Entry(target);
            }
            None => {
                self.clear_selection();
                self.interaction.context_target = ContextTarget::Background;
            }
        }
    }

    pub fn selection_summary(&self) -> (usize, bool, bool) {
        let mut files = 0;
        let mut dirs = 0;
        for r in self.visible.iter() {
            if self.interaction.selected_ids.contains(&r.id) {
                match r.kind {
                    EntryKind::Dir => dirs += 1,
                    _ => files += 1,
                }
            }
        }
        (files + dirs, files > 0, dirs > 0)
    }

    pub fn selected_ids_sorted(&self) -> Vec<String> {
        let mut out = self
            .interaction
            .selected_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        out.sort();
        out
    }

    pub(crate) fn context_entry(&self) -> Option<&str> {
        match &self.interaction.context_target {
            ContextTarget::Entry(id) => Some(id),
            _ => None,
        }
    }

    pub(crate) fn menu_target(&self) -> BrowserMenuTarget {
        match &self.interaction.context_target {
            ContextTarget::Background | ContextTarget::None => BrowserMenuTarget::Background,
            ContextTarget::Entry(target)
                if self.interaction.selected_ids.contains(target)
                    && self.interaction.selected_ids.len() > 1 =>
            {
                let (_, _, has_dir) = self.selection_summary();
                BrowserMenuTarget::Multi { has_dir }
            }
            ContextTarget::Entry(target) => BrowserMenuTarget::Single(
                self.visible
                    .iter()
                    .find(|row| row.id == *target)
                    .map(|row| row.kind)
                    .unwrap_or(EntryKind::File),
            ),
        }
    }

    pub fn target_ids(&self) -> Vec<String> {
        match &self.interaction.context_target {
            ContextTarget::Background => self
                .directory
                .as_ref()
                .map(|dir| vec![dir.clone()])
                .unwrap_or_default(),
            ContextTarget::Entry(target)
                if self.interaction.selected_ids.contains(target)
                    && self.interaction.selected_ids.len() > 1 =>
            {
                self.selected_ids_sorted()
            }
            ContextTarget::Entry(target) => vec![target.clone()],
            ContextTarget::None => self.selected_ids_sorted(),
        }
    }

    pub fn in_trash(&self) -> bool {
        self.directory.as_deref() == Some(trash_files_root())
    }

    pub fn in_recent(&self) -> bool {
        self.directory
            .as_deref()
            .is_some_and(crate::files::recent::is_recent)
    }

    fn publish_view(
        table: &mut TableState<Self>,
        view: RowView,
        cx: &mut Context<TableState<Self>>,
    ) {
        let delegate = table.delegate_mut();
        let old = std::mem::replace(&mut delegate.visible, view);
        // Dropping a superseded large snapshot can also be expensive.
        retire(old, cx);
        delegate
            .interaction
            .selected_ids
            .retain(|id| delegate.visible.contains(id));
        if delegate
            .interaction
            .selection_anchor_id
            .as_ref()
            .is_some_and(|id| !delegate.visible.contains(id))
        {
            delegate.interaction.selection_anchor_id = None;
        }
        if let ContextTarget::Entry(id) = &delegate.interaction.context_target
            && !delegate.visible.contains(id)
        {
            delegate.interaction.context_target = ContextTarget::None;
        }
        if delegate.loading.is_empty()
            && !delegate.has_query()
            && let Some(paths) = delegate.pending_selection.take()
        {
            delegate.interaction.selected_ids = paths
                .into_iter()
                .filter(|path| delegate.visible.contains(path))
                .collect();
            let first = delegate
                .interaction
                .selected_ids
                .iter()
                .filter_map(|id| delegate.visible.position(id))
                .min();
            delegate.interaction.selection_anchor_id =
                first.and_then(|ix| delegate.visible.get(ix).map(|row| row.id.clone()));
            if let Some(index) = first {
                table.scroll_to_row(index, cx);
            }
        }
        cx.notify();
    }

    pub fn set_query(&mut self, query: String, cx: &mut Context<TableState<Self>>) {
        if self.query == query {
            return;
        }
        self.query = query;
        self.start_search(cx);
    }

    fn start_search(&mut self, cx: &mut Context<TableState<Self>>) {
        self.search_job.cancel();
        self.view_job.cancel();
        self.searching = false;
        self.search_truncated = false;
        retire(self.search_rows.take(), cx);
        if !self.has_query() {
            self.rebuild_visible(cx);
            return;
        }
        let Some(root) = self.directory.clone() else {
            return;
        };
        if crate::files::recent::is_recent(&root) {
            self.rebuild_visible(cx);
            return;
        }
        let cancel = self.search_job.start();
        let worker_cancel = cancel.clone();
        self.searching = true;
        let query = self.query.clone();
        let options = self.view_options();
        let origin = self.origin();
        let task = cx.background_executor().spawn(async move {
            let outcome = crate::files::search::search_cancellable(
                std::path::Path::new(&root),
                &query,
                options.show_hidden,
                &worker_cancel,
            )?;
            let rows = RowData::from_entries(outcome.entries, origin, &worker_cancel)?;
            RowView::prepare(rows, options, &worker_cancel).map(|view| (view, outcome.truncated))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |table, cx| {
                let delegate = table.delegate_mut();
                if !delegate.search_job.finish(&cancel) {
                    retire(result, cx);
                    return;
                }
                delegate.searching = false;
                match result {
                    Ok((view, truncated)) => {
                        delegate.search_rows = Some(view.source());
                        delegate.search_truncated = truncated;
                        if view.options == delegate.view_options() {
                            delegate.view_job.cancel();
                            Self::publish_view(table, view, cx);
                        } else {
                            retire(view, cx);
                            delegate.rebuild_visible(cx);
                        }
                    }
                    Err(error) => log::warn!("failed to search directory: {error:#}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn status(&self) -> BrowserStatus {
        BrowserStatus::new(self.visible.len(), self.interaction.selected_ids.len())
            .search_state(self.search_truncated, self.searching && self.has_query())
    }

    fn update_columns(&mut self) {
        let want_original_col = self.in_trash() || self.in_recent();
        if want_original_col != (self.columns.len() > ORIGINAL_COL) {
            self.columns = browser_table_columns(want_original_col);
        }
    }

    pub fn rebuild_visible(&mut self, cx: &mut Context<TableState<Self>>) {
        self.update_columns();
        let source = if self.has_query() {
            self.search_rows.clone().unwrap_or_default()
        } else {
            self.all_rows.clone()
        };
        let options = self.view_options();
        let cancel = self.view_job.start();
        let worker_cancel = cancel.clone();
        let task = cx
            .background_executor()
            .spawn(async move { RowView::prepare(source, options, &worker_cancel) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |table, cx| {
                if !table.delegate_mut().view_job.finish(&cancel) {
                    retire(result, cx);
                    return;
                }
                if let Ok(view) = result {
                    Self::publish_view(table, view, cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub fn row(&self, row_ix: usize) -> Option<&VisibleRow> {
        self.visible.get(row_ix)
    }

    pub fn row_press(&mut self, row_ix: usize, modifiers: gpui_kit::Modifiers) -> bool {
        self.interaction.press_on_row = true;
        self.clear_context_menu_target();
        let Some(row) = self.row(row_ix) else {
            return false;
        };
        let target_id = row.id.clone();

        if modifiers.shift {
            self.range_select_from_anchor(row_ix, &target_id);
            self.interaction.gesture = RowGesture::Idle;
            self.interaction.selection_mode = SelectionMode::Idle;
            return self.interaction.selected_ids.contains(&target_id);
        }

        let already = self.interaction.selected_ids.contains(&target_id);
        self.interaction.selection_anchor_id = Some(target_id.clone());

        if already {
            if modifiers.control {
                self.interaction.selected_ids.remove(&target_id);
                self.interaction.gesture = RowGesture::Idle;
                self.interaction.selection_mode = SelectionMode::Idle;
                return self.interaction.selected_ids.contains(&target_id);
            }
            // Plain click on a row that is part of a multi-selection collapses
            // to just that row, but only once the click finishes — deferring
            // also keeps drag-and-drop on a selected row working.
            self.interaction.gesture = if self.interaction.selected_ids.len() > 1 {
                RowGesture::Deferred
            } else {
                RowGesture::Idle
            };
            self.interaction.selection_mode = SelectionMode::Idle;
            return true;
        }

        if modifiers.control {
            self.interaction.selected_ids.insert(target_id);
        } else {
            self.interaction.selected_ids.clear();
            self.interaction.selected_ids.insert(target_id);
        }
        self.interaction.gesture = RowGesture::Extend;
        self.interaction.selection_mode = SelectionMode::DragSelect;
        true
    }

    pub fn clear_press_on_row(&mut self) {
        self.interaction.press_on_row = false;
    }

    pub fn take_press_on_row(&mut self) -> bool {
        std::mem::take(&mut self.interaction.press_on_row)
    }

    pub fn select_single(&mut self, row_ix: usize) -> Option<usize> {
        let id = self.row(row_ix)?.id.clone();
        self.interaction.selected_ids.clear();
        self.interaction.selected_ids.insert(id);
        self.interaction.selection_anchor_id = self.row(row_ix).map(|row| row.id.clone());
        self.interaction.gesture = RowGesture::Idle;
        Some(row_ix)
    }

    pub fn move_selection(&mut self, delta: i64) -> Option<usize> {
        let count = self.visible.len();
        if count == 0 {
            return None;
        }
        let current = self
            .visible
            .iter()
            .position(|row| self.interaction.selected_ids.contains(&row.id))
            .unwrap_or(0);
        let next = (current as i64 + delta).clamp(0, count as i64 - 1) as usize;
        self.select_single(next)
    }

    pub fn select_all(&mut self) {
        self.interaction.selected_ids = self.visible.iter().map(|row| row.id.clone()).collect();
        self.interaction.selection_anchor_id = self.visible.first().map(|row| row.id.clone());
    }

    fn range_select_from_anchor(&mut self, row_ix: usize, target_id: &str) {
        if let Some(anchor_id) = self.interaction.selection_anchor_id.clone()
            && let Some(anchor_ix) = self.visible.iter().position(|r| r.id == anchor_id)
        {
            let (a, b) = if anchor_ix <= row_ix {
                (anchor_ix, row_ix)
            } else {
                (row_ix, anchor_ix)
            };
            self.interaction.selected_ids.clear();
            for r in self.visible.iter().skip(a).take(b - a + 1) {
                self.interaction.selected_ids.insert(r.id.clone());
            }
        } else {
            self.interaction.selected_ids.clear();
            self.interaction.selected_ids.insert(target_id.to_string());
        }
    }

    pub fn row_release(&mut self, row_ix: usize, modifiers: gpui_kit::Modifiers) -> Option<usize> {
        let deferred = matches!(self.interaction.gesture, RowGesture::Deferred);
        self.interaction.gesture = RowGesture::Idle;
        self.interaction.selection_mode = SelectionMode::Idle;
        if !deferred || self.interaction.dragging {
            return None;
        }
        let row = self.row(row_ix)?;
        let target_id = row.id.clone();
        if modifiers.control || modifiers.shift {
            return None;
        }
        self.interaction.selected_ids.clear();
        self.interaction.selected_ids.insert(target_id.clone());
        self.interaction.selection_anchor_id = Some(target_id);
        Some(row_ix)
    }

    pub fn defer_row_click(&mut self, row_ix: usize) {
        let Some(row) = self.row(row_ix) else {
            return;
        };
        let target_id = row.id.clone();
        if !self.interaction.selected_ids.contains(&target_id) {
            self.interaction.selected_ids.insert(target_id.clone());
        }
        self.interaction.gesture = RowGesture::Deferred;
        self.interaction.selection_anchor_id = Some(target_id);
    }

    pub fn drag_select_row(&mut self, row_ix: usize) -> bool {
        if self.interaction.dragging
            || !matches!(self.interaction.gesture, RowGesture::Extend)
            || self.interaction.selection_mode != SelectionMode::DragSelect
        {
            return false;
        }
        let Some(row) = self.row(row_ix) else {
            return false;
        };
        self.interaction.selected_ids.insert(row.id.clone());
        true
    }

    pub fn set_dragging(&mut self, dragging: bool) {
        self.interaction.dragging = dragging;
        if dragging {
            self.interaction.selection_mode = SelectionMode::Idle;
        }
    }

    pub fn begin_marquee(&mut self) -> HashSet<String> {
        self.clear_context_menu_target();
        self.interaction.selection_anchor_id = None;
        self.interaction.gesture = RowGesture::Idle;
        self.interaction.selection_mode = SelectionMode::Marquee;
        self.interaction.selected_ids.clone()
    }

    pub fn end_marquee(&mut self) {
        if self.interaction.selection_mode == SelectionMode::Marquee {
            self.interaction.selection_mode = SelectionMode::Idle;
        }
    }

    pub fn set_marquee_selection(&mut self, ids: HashSet<String>) {
        self.interaction.context_target = ContextTarget::None;
        self.interaction.selected_ids = ids;
        self.interaction.selection_mode = SelectionMode::Marquee;
    }

    pub fn clear_selection(&mut self) {
        self.interaction.selected_ids.clear();
        self.interaction.selection_anchor_id = None;
        self.interaction.selection_mode = SelectionMode::Idle;
        self.interaction.gesture = RowGesture::Idle;
        self.interaction.dragging = false;
    }

    fn clear_context_menu_target(&mut self) {
        self.interaction.context_target = ContextTarget::None;
    }

    pub(crate) fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    pub(crate) fn set_show_hidden(
        &mut self,
        show_hidden: bool,
        cx: &mut Context<TableState<Self>>,
    ) {
        if self.show_hidden == show_hidden {
            return;
        }
        self.show_hidden = show_hidden;
        if self.has_query() {
            self.start_search(cx);
        } else {
            self.rebuild_visible(cx);
        }
    }

    pub fn has_query(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Re-runs the walk behind a live query. The hidden-file filter is applied
    /// inside the walk, so its results go stale the moment that setting flips.
    pub fn restart_search(&mut self, cx: &mut Context<TableState<Self>>) {
        if self.query.trim().is_empty() {
            return;
        }
        self.start_search(cx);
    }

    pub fn set_sort(&mut self, col: SortColumn, cx: &mut Context<TableState<Self>>) {
        if self.sort.column == col {
            self.sort.direction = match self.sort.direction {
                SortDirection::Asc => SortDirection::Desc,
                SortDirection::Desc => SortDirection::Asc,
            };
        } else {
            self.sort.column = col;
            self.sort.direction = SortDirection::Asc;
        }
        self.rebuild_visible(cx);
        cx.notify();
    }
}

const ORIGINAL_COL: usize = 3;

// Keep final deallocation of large row snapshots off the foreground thread.
fn retire<T: Send + 'static>(old: T, cx: &mut Context<TableState<BrowserDelegate>>) {
    cx.background_executor()
        .spawn(async move {
            drop(old);
        })
        .detach();
}

fn browser_table_columns(show_origin: bool) -> Vec<Column> {
    let mut columns = vec![
        Column::new("name", "Name").width(px(360.0)).resizable(true),
        Column::new("size", "Size")
            .width(px(120.0))
            .text_right()
            .resizable(true),
        Column::new("modified", "Modified")
            .width(px(180.0))
            .resizable(true),
    ];
    if show_origin {
        columns.push(
            Column::new("original", "Deleted from")
                .width(px(280.0))
                .resizable(true),
        );
    }
    columns
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserStatus {
    pub items: usize,
    pub selected: usize,
    /// The listing is the first page of a search that hit its cap.
    pub truncated: bool,
    /// A walk is in flight, so the count will change again.
    pub searching: bool,
}

impl BrowserStatus {
    fn new(items: usize, selected: usize) -> Self {
        Self {
            items,
            selected,
            truncated: false,
            searching: false,
        }
    }

    fn search_state(mut self, truncated: bool, searching: bool) -> Self {
        self.truncated = truncated;
        self.searching = searching;
        self
    }
}
