use std::path::{Path, PathBuf};

use gpui_kit::{App, Entity};

use crate::browser::table::BrowserDelegate;
use gpui_kit::component::table::TableState;

pub(crate) fn target_paths(table: &Entity<TableState<BrowserDelegate>>, cx: &App) -> Vec<PathBuf> {
    table
        .read(cx)
        .delegate()
        .target_ids()
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

pub(crate) fn selected_name(
    table: &Entity<TableState<BrowserDelegate>>,
    cx: &App,
) -> Option<String> {
    let state = table.read(cx);
    let ids = state.delegate().target_ids();
    if ids.len() == 1 {
        state
            .delegate()
            .visible
            .iter()
            .find(|row| row.id == ids[0])
            .map(|row| row.name.clone())
    } else {
        None
    }
}

pub(crate) fn target_dir(
    table: &Entity<TableState<BrowserDelegate>>,
    current_dir: Option<String>,
    cx: &App,
) -> Option<PathBuf> {
    let paths = target_paths(table, cx);
    if paths.len() == 1 {
        let path = &paths[0];
        if path.is_dir() {
            Some(path.clone())
        } else {
            path.parent().map(Path::to_path_buf)
        }
    } else {
        current_dir.map(PathBuf::from)
    }
}
