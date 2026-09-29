use serverus_domain::agent::access::{
    decide, effective_level, is_visible, AccessLevel, Decision, OperationClass,
};

use AccessLevel::{Ask, Full, Off, ReadOnly};
use OperationClass::{Destructive, Exec, Read, Write};

#[test]
fn nothing_set_means_off() {
    assert_eq!(effective_level(None, [None, None], false), Off);
    assert_eq!(effective_level(None, [], false), Off);
}

#[test]
fn own_setting_wins_over_folders() {
    assert_eq!(
        effective_level(Some(ReadOnly), [Some(Full)], false),
        ReadOnly
    );
    // An explicit Off on the connection hides it even inside a Full folder.
    assert_eq!(effective_level(Some(Off), [Some(Full)], false), Off);
}

#[test]
fn nearest_folder_setting_is_inherited() {
    assert_eq!(
        effective_level(None, [None, Some(Ask), Some(Full)], false),
        Ask
    );
    assert_eq!(effective_level(None, [None, None, Some(Full)], false), Full);
}

#[test]
fn full_for_all_overrides_everything() {
    assert_eq!(effective_level(Some(Off), [Some(Off)], true), Full);
    assert_eq!(effective_level(None, [], true), Full);
}

#[test]
fn off_denies_even_reads() {
    for class in [Read, Exec, Write, Destructive] {
        assert_eq!(decide(Off, class, true), Decision::Deny);
    }
    assert!(!is_visible(Off));
    assert!(is_visible(ReadOnly));
}

#[test]
fn read_only_allows_reads_only() {
    assert_eq!(decide(ReadOnly, Read, false), Decision::Allow);
    for class in [Exec, Write, Destructive] {
        // A grant never widens a read-only connection.
        assert_eq!(decide(ReadOnly, class, true), Decision::Deny);
    }
}

#[test]
fn ask_confirms_changes_unless_granted() {
    assert_eq!(decide(Ask, Read, false), Decision::Allow);
    for class in [Exec, Write, Destructive] {
        assert_eq!(decide(Ask, class, false), Decision::Confirm);
        assert_eq!(decide(Ask, class, true), Decision::Allow);
    }
}

#[test]
fn full_never_asks() {
    for class in [Read, Exec, Write, Destructive] {
        assert_eq!(decide(Full, class, false), Decision::Allow);
    }
}

mod creating_connections {
    use serverus_domain::agent::access::{
        created_connection_level, decide_create, decide_new_connection, may_use_as_jump_host,
        AccessLevel, CreateMode, Decision,
    };

    #[test]
    fn the_create_mode_decides() {
        assert_eq!(decide_create(CreateMode::Off, false, true), Decision::Deny);
        assert_eq!(
            decide_create(CreateMode::Ask, false, false),
            Decision::Confirm
        );
        assert_eq!(decide_create(CreateMode::Ask, false, true), Decision::Allow);
        assert_eq!(
            decide_create(CreateMode::Allowed, false, false),
            Decision::Allow
        );
        // Full access for everything includes adding connections.
        assert_eq!(decide_create(CreateMode::Off, true, false), Decision::Allow);
    }

    #[test]
    fn a_new_connection_inherits_but_is_never_hidden_from_its_creator() {
        assert_eq!(
            created_connection_level(AccessLevel::Off),
            Some(AccessLevel::Ask)
        );
        assert_eq!(created_connection_level(AccessLevel::ReadOnly), None);
        assert_eq!(created_connection_level(AccessLevel::Full), None);
    }

    #[test]
    fn a_connection_that_widens_access_needs_the_user() {
        use AccessLevel::{Ask, Full, ReadOnly};
        let allowed = CreateMode::Allowed;
        // Ordinary additions go through without asking.
        assert_eq!(
            decide_new_connection(allowed, false, false, Ask, None),
            Decision::Allow
        );
        assert_eq!(
            decide_new_connection(allowed, false, false, ReadOnly, Some(Full)),
            Decision::Allow
        );
        // Full access from the chosen folder, or tunnelling through an
        // "ask" bastion, is confirmed even where adding is allowed…
        assert_eq!(
            decide_new_connection(allowed, false, false, Full, None),
            Decision::Confirm
        );
        assert_eq!(
            decide_new_connection(allowed, false, false, Ask, Some(Ask)),
            Decision::Confirm
        );
        // …unless the user granted it for a while or gave full access to all.
        assert_eq!(
            decide_new_connection(allowed, false, true, Full, None),
            Decision::Allow
        );
        assert_eq!(
            decide_new_connection(CreateMode::Off, true, false, Full, Some(Ask)),
            Decision::Allow
        );
        // It never turns a refusal into a question.
        assert_eq!(
            decide_new_connection(CreateMode::Off, false, false, Full, None),
            Decision::Deny
        );
        assert_eq!(
            decide_new_connection(CreateMode::Ask, false, false, Ask, None),
            Decision::Confirm
        );
    }

    #[test]
    fn jump_hosts_need_command_rights() {
        assert!(!may_use_as_jump_host(AccessLevel::Off));
        assert!(!may_use_as_jump_host(AccessLevel::ReadOnly));
        assert!(may_use_as_jump_host(AccessLevel::Ask));
        assert!(may_use_as_jump_host(AccessLevel::Full));
    }
}
