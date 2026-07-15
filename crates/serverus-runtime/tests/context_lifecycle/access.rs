use std::sync::Arc;

use serverus_application::context::ContextEvent;
use serverus_domain::runtime_context::{VaultAccess, VaultKey};
use serverus_runtime::{ApplicationHandle, RuntimeError};

use super::support::{handle, FixedId, NoopCleanup, RecordedEvents};

#[test]
fn unlocked_lease_captures_the_authorizing_vault_identity() {
    let handle = handle();
    let vault = VaultKey::new("primary").unwrap();
    handle.activate_vault(vault.clone()).unwrap();

    let lease = handle.require_unlocked().unwrap();

    assert_eq!(lease.vault(), &vault);
}

#[test]
fn locking_preserves_the_context_but_revokes_unlocked_access() {
    let handle = handle();
    let vault = VaultKey::new("primary").unwrap();
    let id = handle.activate_vault(vault.clone()).unwrap();
    let active_lease = handle.require_active().unwrap();
    let unlocked_lease = handle.require_unlocked().unwrap();
    let old_access_epoch = unlocked_lease.vault_access_epoch().unwrap();
    assert_eq!(active_lease.vault_access_epoch(), None);

    assert_eq!(handle.lock_vault().unwrap(), id);
    assert_eq!(handle.require_unlocked(), Err(RuntimeError::VaultLocked));
    assert_eq!(handle.require_active().unwrap().context_id(), id);
    assert_eq!(active_lease.validate(&handle), Ok(()));
    assert!(!active_lease.is_cancelled());
    assert_eq!(
        unlocked_lease.validate(&handle),
        Err(RuntimeError::StaleContext)
    );
    assert!(unlocked_lease.is_cancelled());

    assert_eq!(handle.activate_vault(vault).unwrap(), id);
    assert_eq!(active_lease.validate(&handle), Ok(()));
    assert_eq!(
        unlocked_lease.validate(&handle),
        Err(RuntimeError::StaleContext)
    );
    let new_unlocked_lease = handle.require_unlocked().unwrap();
    assert_eq!(new_unlocked_lease.validate(&handle), Ok(()));
    assert_ne!(
        new_unlocked_lease.vault_access_epoch(),
        Some(old_access_epoch)
    );
}

#[test]
fn reactivating_the_same_vault_reuses_its_generation_and_publishes_access() {
    let events = RecordedEvents::default();
    let handle = ApplicationHandle::new(
        Arc::new(FixedId),
        Arc::new(NoopCleanup),
        Arc::new(events.clone()),
    );
    let vault = VaultKey::new("primary").unwrap();

    let id = handle.activate_vault(vault.clone()).unwrap();
    handle.lock_vault().unwrap();
    let reactivated = handle.activate_vault(vault.clone()).unwrap();

    assert_eq!(reactivated, id);
    assert_eq!(
        events.snapshot(),
        vec![
            ContextEvent::Activated {
                context_id: id,
                vault,
            },
            ContextEvent::AccessChanged {
                context_id: id,
                access: VaultAccess::Locked,
            },
            ContextEvent::AccessChanged {
                context_id: id,
                access: VaultAccess::Unlocked,
            },
        ]
    );
}

#[test]
fn moving_the_active_vault_keeps_its_generation_but_changes_its_key() {
    let handle = handle();
    let id = handle
        .activate_vault(VaultKey::new("old-path").unwrap())
        .unwrap();

    let moved = handle
        .reidentify_vault(VaultKey::new("new-path").unwrap())
        .unwrap();

    assert_eq!(moved, id);
    assert_eq!(
        handle.activate_vault(VaultKey::new("new-path").unwrap()),
        Ok(id)
    );
    assert_eq!(
        handle.activate_vault(VaultKey::new("old-path").unwrap()),
        Err(RuntimeError::DifferentVaultActive)
    );
}
