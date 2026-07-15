use serverus_domain::runtime_context::{RuntimeContext, RuntimeContextId, VaultAccess, VaultKey};

fn context_id() -> RuntimeContextId {
    RuntimeContextId::try_from(7_u128).expect("test ID is non-zero")
}

#[test]
fn runtime_context_id_rejects_zero() {
    assert!(RuntimeContextId::try_from(0_u128).is_err());
}

#[test]
fn vault_key_rejects_an_empty_identity() {
    assert!(VaultKey::new("").is_err());
}

#[test]
fn locking_and_unlocking_preserve_context_generation() {
    let vault = VaultKey::new("/vaults/main.serverus").unwrap();
    let context = RuntimeContext::unlocked(context_id(), vault.clone());

    let locked = context.lock();
    assert_eq!(locked.id(), context_id());
    assert_eq!(locked.vault(), &vault);
    assert_eq!(locked.access(), VaultAccess::Locked);

    let unlocked = locked.unlock();
    assert_eq!(unlocked.id(), context_id());
    assert_eq!(unlocked.vault(), &vault);
    assert_eq!(unlocked.access(), VaultAccess::Unlocked);
}

#[test]
fn changing_the_vault_key_preserves_the_context_generation() {
    let context = RuntimeContext::unlocked(context_id(), VaultKey::new("old-path").unwrap());

    let moved = context.with_vault(VaultKey::new("new-path").unwrap());

    assert_eq!(moved.id(), context_id());
    assert_eq!(moved.vault().as_str(), "new-path");
    assert_eq!(moved.access(), VaultAccess::Unlocked);
}
