//! Display-filter tests: hidden dot-entries and the local junk setting.

use std::fs;

use super::{
    entry, ignore_mtime, join_remote, local_tree, matching_remote, status, status_filtered,
    CompareFilter, SubtreeStatus, REMOTE,
};

#[tokio::test]
async fn hidden_entries_follow_the_pane_filter() {
    let local = local_tree();
    fs::write(local.path().join("sub/.DS_Store"), b"junk").unwrap();
    assert_eq!(
        status(local.path(), &matching_remote(), ignore_mtime(), false).await,
        SubtreeStatus::Matching
    );
    assert_eq!(
        status(local.path(), &matching_remote(), ignore_mtime(), true).await,
        SubtreeStatus::Different
    );
}

#[tokio::test]
async fn local_junk_is_ignored_only_when_the_setting_says_so() {
    let local = local_tree();
    fs::write(local.path().join("sub/.DS_Store"), b"junk").unwrap();
    fs::write(local.path().join("thumbs.db"), b"junk").unwrap();
    let junk_hidden = CompareFilter {
        include_hidden: true,
        hide_local_junk: true,
    };
    assert_eq!(
        status_filtered(
            local.path(),
            &matching_remote(),
            ignore_mtime(),
            junk_hidden
        )
        .await,
        SubtreeStatus::Matching
    );
    // Without the setting the same files are honest local-only differences.
    let junk_visible = CompareFilter {
        include_hidden: true,
        hide_local_junk: false,
    };
    assert_eq!(
        status_filtered(
            local.path(),
            &matching_remote(),
            ignore_mtime(),
            junk_visible
        )
        .await,
        SubtreeStatus::Different
    );
}

#[tokio::test]
async fn remote_junk_still_counts_as_a_difference() {
    // The filter is local-only: a stray .DS_Store on the server must keep
    // showing up so the user can find and delete it.
    let local = local_tree();
    let mut remote = matching_remote();
    remote
        .dirs
        .get_mut(&join_remote(REMOTE, "sub"))
        .unwrap()
        .push(entry(".DS_Store", false, 4, None));
    let filter = CompareFilter {
        include_hidden: true,
        hide_local_junk: true,
    };
    assert_eq!(
        status_filtered(local.path(), &remote, ignore_mtime(), filter).await,
        SubtreeStatus::Different
    );
}
