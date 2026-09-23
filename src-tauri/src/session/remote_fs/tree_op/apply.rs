//! Apply phase: files first (bulk where the protocol has it, otherwise
//! bounded-parallel single requests), then directories deepest-first.

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use futures::future::BoxFuture;
use futures::StreamExt;

use crate::error::AppResult;
use crate::session::remote_fs::{parent_remote, RemoteFs, BULK_DELETE_MAX};

use super::scan::TreePlan;
use super::{failure_error, Checkpoint, EntryFailure, TreeAction, TreeOutcome, TreeProgress};

#[derive(Clone, Copy)]
enum Step {
    DeleteFile,
    DeleteDir,
    Chmod(u32),
}

async fn run_step(fs: &dyn RemoteFs, step: Step, path: &str) -> AppResult<()> {
    match step {
        Step::DeleteFile => fs.delete_file(path).await,
        Step::DeleteDir => fs.delete_dir(path).await,
        Step::Chmod(mode) => fs.chmod(path, mode).await,
    }
}

struct Run<'a> {
    fs: &'a dyn RemoteFs,
    progress: TreeProgress<'a>,
    checkpoint: &'a dyn Checkpoint,
    failures: Vec<EntryFailure>,
}

pub(super) async fn apply(
    fs: &dyn RemoteFs,
    root: &str,
    plan: TreePlan,
    action: TreeAction,
    progress: TreeProgress<'_>,
    checkpoint: &dyn Checkpoint,
) -> AppResult<TreeOutcome> {
    let (file_step, dir_step, with_files, with_dirs) = match action {
        TreeAction::Delete => (Step::DeleteFile, Step::DeleteDir, true, true),
        TreeAction::Chmod { mode, files, dirs } => {
            (Step::Chmod(mode), Step::Chmod(mode), files, dirs)
        }
    };
    let files: Vec<String> = plan
        .files
        .into_iter()
        .filter(|file| with_files && !(file.is_symlink && action != TreeAction::Delete))
        .map(|file| file.path)
        .collect();
    let levels = if with_dirs {
        by_depth(plan.dirs)
    } else {
        Vec::new()
    };
    let total = files.len() + levels.iter().map(Vec::len).sum::<usize>();
    progress.total.store(total as u64, Ordering::Relaxed);

    let mut run = Run {
        fs,
        progress,
        checkpoint,
        failures: plan.failures,
    };
    let finished = if action == TreeAction::Delete {
        run.delete_files(&files).await
    } else {
        run.each(&files, file_step).await
    };
    if !finished {
        return Ok(TreeOutcome::Cancelled);
    }
    for level in levels {
        let level = if action == TreeAction::Delete {
            // A directory whose contents were not fully removed cannot go.
            let blocked = blocked_dirs(root, &run.failures);
            let (kept, skipped): (Vec<_>, Vec<_>) = level
                .into_iter()
                .partition(|dir| !blocked.contains(dir.trim_end_matches('/')));
            run.progress
                .done
                .fetch_add(skipped.len() as u64, Ordering::Relaxed);
            kept
        } else {
            level
        };
        if !run.each(&level, dir_step).await {
            return Ok(TreeOutcome::Cancelled);
        }
    }
    if run.failures.is_empty() {
        Ok(TreeOutcome::Completed)
    } else {
        Err(failure_error(action, &run.failures, total as u64))
    }
}

impl Run<'_> {
    /// Bulk requests while the protocol accepts them; single requests from
    /// the first refusal on (a provider without `DeleteObjects` support
    /// answers with an error, not with `Ok(None)`).
    async fn delete_files(&mut self, files: &[String]) -> bool {
        let mut bulk = true;
        for chunk in files.chunks(BULK_DELETE_MAX) {
            if !self.checkpoint.proceed().await {
                return false;
            }
            if bulk {
                match self.fs.delete_files_bulk(chunk).await {
                    Ok(Some(failures)) => {
                        self.failures.extend(failures);
                        self.progress
                            .done
                            .fetch_add(chunk.len() as u64, Ordering::Relaxed);
                        continue;
                    }
                    Ok(None) | Err(_) => bulk = false,
                }
            }
            if !self.each(chunk, Step::DeleteFile).await {
                return false;
            }
        }
        true
    }

    /// Up to `parallel_ops` requests in flight; the checkpoint is polled
    /// after every completion, so pause/cancel react within one round trip.
    async fn each(&mut self, paths: &[String], step: Step) -> bool {
        let fs = self.fs;
        // Windows bound the boxed futures alive at once on huge trees.
        for window in paths.chunks(BULK_DELETE_MAX) {
            // Boxed and collected up front: a closure over borrowed paths in
            // the stream type trips the higher-ranked `Send` check of callers.
            let requests: Vec<BoxFuture<'_, (&String, AppResult<()>)>> = window
                .iter()
                .map(|path| Box::pin(async move { (path, run_step(fs, step, path).await) }) as _)
                .collect();
            let mut results =
                futures::stream::iter(requests).buffer_unordered(fs.parallel_ops().max(1));
            while let Some((path, result)) = results.next().await {
                self.progress.done.fetch_add(1, Ordering::Relaxed);
                if let Err(error) = result {
                    self.failures.push(EntryFailure::new(path, &error));
                }
                if !self.checkpoint.proceed().await {
                    return false;
                }
            }
        }
        true
    }
}

/// Directories grouped by depth, deepest level first.
fn by_depth(dirs: Vec<(usize, String)>) -> Vec<Vec<String>> {
    let max_depth = dirs.iter().map(|(depth, _)| *depth).max();
    let mut levels = vec![Vec::new(); max_depth.map_or(0, |depth| depth + 1)];
    for (depth, dir) in dirs {
        levels[depth].push(dir);
    }
    levels.reverse();
    levels
}

/// Every directory between a failed entry and the root, inclusive.
fn blocked_dirs(root: &str, failures: &[EntryFailure]) -> HashSet<String> {
    let root = root.trim_end_matches('/');
    let mut blocked = HashSet::new();
    for failure in failures {
        // A failed directory blocks itself (it is still there), a failed
        // file only its ancestors; blocking the path itself is harmless.
        let mut current = failure.path.trim_end_matches('/').to_string();
        loop {
            let at_root = current.len() <= root.len();
            if !blocked.insert(current.clone()) || at_root {
                break;
            }
            current = parent_remote(&current);
        }
    }
    blocked
}
