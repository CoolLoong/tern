#[cfg(test)]
mod tests;

use crate::files::operations::trash_paths;
use gpui_kit::AsyncApp;
use gpui_transfer::{
    TransferCenterState, TransferFailure, TransferProgress, TransferStatus, TransferTask,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub static NEXT_GROUP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

pub fn next_group_id() -> String {
    format!(
        "tern-ops-{}",
        NEXT_GROUP.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

/// Starts a transfer from an event handler, where `App` is already borrowed
/// and `AsyncApp::update` would panic. The first step is deferred so it runs
/// once the event loop released the `App` and can be updated again.
pub fn start_transfer(
    sources: Vec<PathBuf>,
    dst_dir: String,
    group_id: String,
    label: &'static str,
    op: impl Fn(&Path, &Path, &AtomicBool) -> std::io::Result<()> + Send + Sync + 'static,
    cx: &mut gpui_kit::App,
) {
    let total = sources.len();
    let cancel = Arc::new(AtomicBool::new(false));
    let task = TransferTask::new(&group_id, label)
        .with_group(&group_id, Some(total))
        .with_kind(gpui_transfer::TransferKind::Other)
        .with_cancel_token(cancel.clone());
    let async_cx = cx.to_async();
    cx.foreground_executor()
        .spawn(async move {
            let mut cx = async_cx;
            run_transfer(
                sources, dst_dir, group_id, label, op, task, cancel, total, &mut cx,
            );
        })
        .detach();
}

#[allow(clippy::too_many_arguments)]
fn run_transfer(
    sources: Vec<PathBuf>,
    dst_dir: String,
    group_id: String,
    label: &'static str,
    op: impl Fn(&Path, &Path, &AtomicBool) -> std::io::Result<()> + Send + Sync + 'static,
    task: TransferTask,
    cancel: Arc<AtomicBool>,
    total: usize,
    cx: &mut AsyncApp,
) {
    cx.update(|cx| {
        cx.global_mut::<TransferCenterState>().upsert(task);
    });

    let dst = PathBuf::from(&dst_dir);
    let worker_cancel = cancel;
    let worker_sources = sources;
    let worker = cx.background_executor().spawn(async move {
        worker_sources
            .iter()
            .map(|src| (src.clone(), op(src, &dst, &worker_cancel)))
            .collect::<Vec<_>>()
    });
    cx.spawn(async move |cx| {
        let mut failures = 0usize;
        for (done, (src, result)) in worker.await.into_iter().enumerate() {
            let status = match result {
                Ok(()) => TransferStatus::Finished,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => TransferStatus::Cancelled,
                Err(e) => {
                    log::warn!("{} failed {}: {e:#}", label, src.display());
                    failures += 1;
                    TransferStatus::Failed
                }
            };
            let done = done + 1;
            cx.update(|cx| {
                let state = cx.global_mut::<TransferCenterState>();
                state.upsert(
                    TransferTask::new(format!("{group_id}:{done}"), label)
                        .with_group(&group_id, Some(total))
                        .with_kind(gpui_transfer::TransferKind::Other)
                        .with_status(status)
                        .with_progress(TransferProgress::Determinate(
                            done as f32 / total.max(1) as f32,
                        )),
                );
            });
        }
        cx.update(|cx| {
            let state = cx.global_mut::<TransferCenterState>();
            state.remove_group(&group_id);
            if failures > 0 {
                state.note_failure(TransferFailure {
                    label: label.trim_end_matches('…').to_string(),
                    count: failures,
                });
            }
            state.note_completed();
        });
    })
    .detach();
}

pub(crate) fn start_file_operation(
    label: &'static str,
    operation: impl FnOnce() -> usize + Send + 'static,
    cx: &mut gpui_kit::App,
) {
    let group_id = next_group_id();
    let cx = cx.to_async();
    cx.spawn(async move |cx| {
        cx.update(|cx| {
            cx.global_mut::<TransferCenterState>().upsert(
                TransferTask::new(&group_id, format!("{label}…")).with_group(&group_id, Some(1)),
            );
        });
        let worker = cx.background_executor().spawn(async move { operation() });
        let failed = worker.await;
        cx.update(|cx| {
            let state = cx.global_mut::<TransferCenterState>();
            state.remove_group(&group_id);
            if failed > 0 {
                state.note_failure(TransferFailure {
                    label: label.to_string(),
                    count: failed,
                });
            }
            state.note_completed();
        });
    })
    .detach();
}

pub(crate) fn start_path_operation(
    sources: Vec<PathBuf>,
    label: &'static str,
    operation: fn(&Path) -> std::io::Result<()>,
    cx: &mut gpui_kit::App,
) {
    if sources.is_empty() {
        return;
    }
    start_file_operation(
        label,
        move || {
            sources
                .into_iter()
                .filter(|path| {
                    if let Err(error) = operation(path) {
                        log::warn!("{label} failed {}: {error:#}", path.display());
                        true
                    } else {
                        false
                    }
                })
                .count()
        },
        cx,
    );
}

pub fn move_to_trash(sources: Arc<[PathBuf]>, cx: &mut gpui_kit::App) {
    if !sources.is_empty() {
        start_file_operation("Move to Trash", move || trash_paths(&sources).len(), cx);
    }
}

#[allow(dead_code)]
pub fn cancel_group(cx: &mut gpui_kit::App, group_id: &str) {
    let state = cx.global_mut::<TransferCenterState>();
    for task in state.tasks_sorted() {
        if task.group_id.as_deref() == Some(group_id)
            && let Some(token) = task.cancel
        {
            token.store(true, Ordering::Relaxed);
        }
    }
}
