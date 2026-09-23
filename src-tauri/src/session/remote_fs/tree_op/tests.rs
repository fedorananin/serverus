mod mem_fs;

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use crate::error::AppResult;

use super::{run_tree_action, Checkpoint, TreeAction, TreeOutcome, TreeProgress, Unattended};
use mem_fs::{Bulk, Kind, MemFs};

fn site() -> MemFs {
    MemFs::with(&[
        ("/srv", Kind::Dir),
        ("/srv/keep.txt", Kind::File),
        ("/srv/site", Kind::Dir),
        ("/srv/site/index.html", Kind::File),
        ("/srv/site/assets", Kind::Dir),
        ("/srv/site/assets/app.js", Kind::File),
        ("/srv/site/assets/img", Kind::Dir),
        ("/srv/site/assets/img/a.png", Kind::File),
        ("/srv/site/assets/img/b.png", Kind::File),
        ("/srv/site/empty", Kind::Dir),
    ])
}

#[derive(Default)]
struct Counters {
    total: AtomicU64,
    done: AtomicU64,
    scanning: AtomicBool,
}

impl Counters {
    fn progress(&self) -> TreeProgress<'_> {
        TreeProgress {
            total: &self.total,
            done: &self.done,
            scanning: &self.scanning,
        }
    }
    fn read(&self) -> (u64, u64) {
        (
            self.total.load(Ordering::SeqCst),
            self.done.load(Ordering::SeqCst),
        )
    }
}

async fn run(fs: &MemFs, root: &str, action: TreeAction) -> (AppResult<TreeOutcome>, Counters) {
    let counters = Counters::default();
    let result = run_tree_action(fs, root, true, action, counters.progress(), &Unattended).await;
    (result, counters)
}

#[tokio::test]
async fn deletes_a_nested_tree_with_exact_progress() {
    let fs = site();
    let (result, counters) = run(&fs, "/srv/site", TreeAction::Delete).await;
    assert_eq!(result.unwrap(), TreeOutcome::Completed);
    assert_eq!(fs.paths(), vec!["/srv", "/srv/keep.txt"]);
    // 4 files + 4 directories (root included).
    assert_eq!(counters.read(), (8, 8));
    assert!(!counters.scanning.load(Ordering::SeqCst));
}

#[tokio::test]
async fn one_failure_does_not_stop_the_rest() {
    let mut fs = site();
    fs.deny.insert("/srv/site/assets/img/a.png".into());
    let (result, counters) = run(&fs, "/srv/site", TreeAction::Delete).await;
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("1 of 8 entries could not be deleted"),
        "{message}"
    );
    assert!(message.contains("a.png: permission denied"), "{message}");
    // Only the failed file and its ancestors remain; siblings are gone.
    assert_eq!(
        fs.paths(),
        vec![
            "/srv",
            "/srv/keep.txt",
            "/srv/site",
            "/srv/site/assets",
            "/srv/site/assets/img",
            "/srv/site/assets/img/a.png",
        ]
    );
    assert_eq!(counters.read(), (8, 8));
}

#[tokio::test]
async fn directory_symlinks_are_removed_not_descended() {
    let fs = MemFs::with(&[
        ("/srv", Kind::Dir),
        ("/srv/site", Kind::Dir),
        ("/srv/site/shared", Kind::DirLink),
        ("/srv/shared-target", Kind::Dir),
        ("/srv/shared-target/data.db", Kind::File),
    ]);
    let (result, _) = run(&fs, "/srv/site", TreeAction::Delete).await;
    assert_eq!(result.unwrap(), TreeOutcome::Completed);
    assert_eq!(
        fs.paths(),
        vec!["/srv", "/srv/shared-target", "/srv/shared-target/data.db"]
    );
}

#[tokio::test]
async fn refuses_to_delete_the_root() {
    let fs = site();
    let (result, _) = run(&fs, "/", TreeAction::Delete).await;
    assert!(result.is_err());
    assert_eq!(fs.paths().len(), 10);
}

#[tokio::test]
async fn single_file_root_is_deleted_directly() {
    let fs = site();
    let counters = Counters::default();
    let result = run_tree_action(
        &fs,
        "/srv/keep.txt",
        false,
        TreeAction::Delete,
        counters.progress(),
        &Unattended,
    )
    .await;
    assert_eq!(result.unwrap(), TreeOutcome::Completed);
    assert!(!fs.paths().contains(&"/srv/keep.txt".to_string()));
    assert_eq!(counters.read(), (1, 1));
}

struct CancelAfter(AtomicUsize);

#[async_trait::async_trait]
impl Checkpoint for CancelAfter {
    async fn proceed(&self) -> bool {
        self.0
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok()
    }
}

#[tokio::test]
async fn cancel_stops_between_requests() {
    let fs = site();
    let counters = Counters::default();
    // Scan checkpoints: one up front plus one per listed directory (4).
    let checkpoint = CancelAfter(AtomicUsize::new(5 + 2));
    let result = run_tree_action(
        &fs,
        "/srv/site",
        true,
        TreeAction::Delete,
        counters.progress(),
        &checkpoint,
    )
    .await;
    assert_eq!(result.unwrap(), TreeOutcome::Cancelled);
    let left = fs.paths().len();
    assert!(left > 2 && left < 10, "{:?}", fs.paths());
}

#[tokio::test]
async fn bulk_delete_is_used_and_reports_failures() {
    let mut fs = site();
    fs.bulk = Bulk::Supported;
    fs.deny.insert("/srv/site/index.html".into());
    let (result, counters) = run(&fs, "/srv/site", TreeAction::Delete).await;
    let message = result.unwrap_err().to_string();
    assert!(message.contains("index.html: AccessDenied"), "{message}");
    assert_eq!(fs.bulk_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        fs.paths(),
        vec!["/srv", "/srv/keep.txt", "/srv/site", "/srv/site/index.html"]
    );
    assert_eq!(counters.read(), (8, 8));
}

#[tokio::test]
async fn refused_bulk_falls_back_to_single_deletes() {
    let mut fs = site();
    fs.bulk = Bulk::Refused;
    let (result, _) = run(&fs, "/srv/site", TreeAction::Delete).await;
    assert_eq!(result.unwrap(), TreeOutcome::Completed);
    assert_eq!(fs.paths(), vec!["/srv", "/srv/keep.txt"]);
}

#[tokio::test]
async fn snapshot_scan_removes_implied_directories() {
    let mut fs = site();
    fs.snapshot = true;
    // Object stores have no empty directories without a placeholder.
    fs.nodes.lock().unwrap().remove("/srv/site/empty");
    let (result, counters) = run(&fs, "/srv/site", TreeAction::Delete).await;
    assert_eq!(result.unwrap(), TreeOutcome::Completed);
    assert_eq!(fs.paths(), vec!["/srv", "/srv/keep.txt"]);
    // 4 files + root + assets + assets/img — no listing calls needed.
    assert_eq!(counters.read().0, 7);
}

#[tokio::test]
async fn requests_run_in_parallel_up_to_the_protocol_limit() {
    let mut fs = site();
    fs.parallel = 3;
    let (result, _) = run(&fs, "/srv/site", TreeAction::Delete).await;
    assert_eq!(result.unwrap(), TreeOutcome::Completed);
    assert_eq!(fs.max_in_flight.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn recursive_chmod_honours_scope_and_skips_symlinks() {
    let fs = MemFs::with(&[
        ("/srv", Kind::Dir),
        ("/srv/a.txt", Kind::File),
        ("/srv/sub", Kind::Dir),
        ("/srv/sub/b.txt", Kind::File),
        ("/srv/link", Kind::DirLink),
    ]);
    let files_only = TreeAction::Chmod {
        mode: 0o640,
        files: true,
        dirs: false,
    };
    let (result, counters) = run(&fs, "/srv", files_only).await;
    assert_eq!(result.unwrap(), TreeOutcome::Completed);
    let modes = fs.modes.lock().unwrap().clone();
    assert_eq!(
        modes.keys().collect::<Vec<_>>(),
        vec!["/srv/a.txt", "/srv/sub/b.txt"]
    );
    assert_eq!(counters.read(), (2, 2));

    let dirs_only = TreeAction::Chmod {
        mode: 0o750,
        files: false,
        dirs: true,
    };
    let (result, _) = run(&fs, "/srv", dirs_only).await;
    assert_eq!(result.unwrap(), TreeOutcome::Completed);
    let modes = fs.modes.lock().unwrap().clone();
    assert_eq!(modes.get("/srv"), Some(&0o750));
    assert_eq!(modes.get("/srv/sub"), Some(&0o750));
    assert!(!modes.contains_key("/srv/link"));
}
