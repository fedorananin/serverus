use crate::agent::control::ControlRegistry;
use crate::agent::exec::StartedCommand;

fn started(nonce: &str, command: &str) -> StartedCommand {
    StartedCommand {
        command: command.into(),
        nonce: nonce.into(),
        offset: 0,
        prompt: "$".into(),
        interrupts: 0,
        flavor: serverus_domain::agent::shell::ShellFlavor::Posix,
    }
}

#[test]
fn a_running_command_is_visible_until_it_finishes() {
    let registry = ControlRegistry::default();
    let (_handles, event) = registry.begin("t1", "s1", started("n1", "make"));
    assert_eq!(event.running.as_deref(), Some("make"));
    assert_eq!(registry.running("t1").unwrap().started.nonce, "n1");

    let event = registry.finish("t1", "n1").unwrap();
    assert_eq!(event.running, None);
    assert!(registry.running("t1").is_none());
}

#[test]
fn a_stale_finish_does_not_clear_a_newer_command() {
    let registry = ControlRegistry::default();
    let (mut first, _) = registry.begin("t1", "s1", started("n1", "sleep 100"));
    let (_second, _) = registry.begin("t1", "s1", started("n2", "ls"));
    // Replacing the command stops following the first one.
    assert!(matches!(
        first.abandoned.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Closed)
    ));
    assert!(registry.finish("t1", "n1").is_none());
    assert_eq!(registry.running("t1").unwrap().started.nonce, "n2");
}

#[test]
fn abandoning_signals_the_follower() {
    let registry = ControlRegistry::default();
    let (mut handles, _) = registry.begin("t1", "s1", started("n1", "tail -f log"));
    assert!(matches!(
        handles.abandoned.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    let event = registry.abandon("t1").unwrap();
    assert_eq!(event.running, None);
    assert!(matches!(
        handles.abandoned.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Closed)
    ));
}

#[test]
fn user_control_toggles_and_survives_commands() {
    let registry = ControlRegistry::default();
    assert!(!registry.user_control("t1"));
    let event = registry.set_user_control("t1", "s1", true);
    assert!(event.user_control);
    assert_eq!(event.session_id, "s1");
    assert!(registry.user_control("t1"));
    let (_handles, event) = registry.begin("t1", "s1", started("n1", "ls"));
    assert!(event.user_control);
    registry.set_user_control("t1", "s1", false);
    assert!(!registry.user_control("t1"));
}

#[test]
fn a_closed_session_is_forgotten() {
    let registry = ControlRegistry::default();
    registry.set_user_control("t1", "s1", true);
    registry.set_user_control("t2", "s2", true);
    registry.forget_session("s1");
    let terms: Vec<String> = registry.snapshot().into_iter().map(|e| e.term_id).collect();
    assert_eq!(terms, vec!["t2".to_string()]);
}

#[tokio::test]
async fn typing_into_one_terminal_is_serialised() {
    let registry = ControlRegistry::default();
    let lock = registry.typing_lock("t1", "s1");
    let held = lock.clone().lock_owned().await;
    // A second caller for the same terminal waits; another terminal does not.
    assert!(registry.typing_lock("t1", "s1").try_lock().is_err());
    assert!(registry.typing_lock("t2", "s1").try_lock().is_ok());
    drop(held);
    assert!(registry.typing_lock("t1", "s1").try_lock().is_ok());
}

#[test]
fn a_shell_override_belongs_to_its_terminal() {
    use serverus_domain::agent::shell::ShellFlavor;
    let registry = ControlRegistry::default();
    assert_eq!(registry.shell("t1"), None);
    registry.set_shell("t1", "s1", ShellFlavor::Fish);
    assert_eq!(registry.shell("t1"), Some(ShellFlavor::Fish));
    registry.forget_session("s1");
    assert_eq!(registry.shell("t1"), None);
}
