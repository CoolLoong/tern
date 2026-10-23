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

pub(crate) fn open_files(paths: &[PathBuf]) -> usize {
    open_files_with(paths, |path| {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .is_ok()
    })
}

fn open_files_with(paths: &[PathBuf], mut launch: impl FnMut(&Path) -> bool) -> usize {
    let mut spawned = 0;
    for path in paths {
        if launch(path) {
            spawned += 1;
        } else {
            log::warn!("failed to open {}", path.display());
        }
    }
    spawned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_files_with_missing_command_does_not_panic() {
        let paths = vec![PathBuf::from("/nonexistent/xyz")];
        // Inject a failed launcher so the test is independent of host PATH and
        // never starts a real desktop application.
        let spawned = open_files_with(&paths, |_| false);
        assert_eq!(spawned, 0);
    }
}
