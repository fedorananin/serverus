use std::fmt;
use std::num::NonZeroU128;
use std::num::NonZeroU64;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeContextId(NonZeroU128);

impl RuntimeContextId {
    pub const fn get(self) -> u128 {
        self.0.get()
    }
}

impl TryFrom<u128> for RuntimeContextId {
    type Error = InvalidRuntimeContextId;

    fn try_from(value: u128) -> Result<Self, Self::Error> {
        NonZeroU128::new(value)
            .map(Self)
            .ok_or(InvalidRuntimeContextId)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidRuntimeContextId;

impl fmt::Display for InvalidRuntimeContextId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a runtime context ID must be non-zero")
    }
}

impl std::error::Error for InvalidRuntimeContextId {}

/// Monotonic secret-access authorization within one runtime context.
/// The pair `(RuntimeContextId, VaultAccessEpoch)` identifies one unlock.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct VaultAccessEpoch(NonZeroU64);

impl VaultAccessEpoch {
    pub const fn initial() -> Self {
        Self(NonZeroU64::MIN)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub fn next(self) -> Option<Self> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct VaultKey(String);

impl VaultKey {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidVaultKey> {
        let value = value.into();
        if value.is_empty() {
            return Err(InvalidVaultKey);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidVaultKey;

impl fmt::Display for InvalidVaultKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a vault key must not be empty")
    }
}

impl std::error::Error for InvalidVaultKey {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VaultAccess {
    Locked,
    Unlocked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeContext {
    id: RuntimeContextId,
    vault: VaultKey,
    access: VaultAccess,
}

impl RuntimeContext {
    pub const fn unlocked(id: RuntimeContextId, vault: VaultKey) -> Self {
        Self {
            id,
            vault,
            access: VaultAccess::Unlocked,
        }
    }

    pub const fn id(&self) -> RuntimeContextId {
        self.id
    }

    pub const fn access(&self) -> VaultAccess {
        self.access
    }

    pub fn vault(&self) -> &VaultKey {
        &self.vault
    }

    pub fn lock(&self) -> Self {
        Self {
            access: VaultAccess::Locked,
            ..self.clone()
        }
    }

    pub fn unlock(&self) -> Self {
        Self {
            access: VaultAccess::Unlocked,
            ..self.clone()
        }
    }

    pub fn with_vault(&self, vault: VaultKey) -> Self {
        Self {
            vault,
            ..self.clone()
        }
    }
}
