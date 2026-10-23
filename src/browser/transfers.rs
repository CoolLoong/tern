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

/// Starts a delete from an event handler. See [`start_transfer`] for why the
/// first step cannot touch the `App` synchronously.
pub fn move_to_trash(sources: Vec<PathBuf>, group_id: String, cx: &mut gpui_kit::App) {
    let cx = cx.to_async();
    cx.spawn(async move |cx| {
        let worker = cx
            .background_executor()
            .spawn(async move { trash_paths(&sources) });
        let failed = worker.await;
        if !failed.is_empty() {
            log::warn!("trash failed for {} items", failed.len());
        }
        cx.update(|cx| {
            let state = cx.global_mut::<TransferCenterState>();
            state.remove_group(&group_id);
            if !failed.is_empty() {
                state.note_failure(TransferFailure {
                    label: "Move to Trash".to_string(),
                    count: failed.len(),
                });
            }
            state.note_completed();
        });
    })
    .detach();
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
