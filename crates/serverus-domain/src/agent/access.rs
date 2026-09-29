//! Agent access policy.
//!
//! Every connection resolves to one [`AccessLevel`]: its own setting, else
//! the nearest enclosing folder's, else [`AccessLevel::Off`]. A global
//! "full access for everything" switch overrides all of them. Each agent
//! request belongs to one [`OperationClass`], and [`decide`] turns the pair
//! into allow / ask the user / deny.

/// How much an agent may do on one connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccessLevel {
    /// The connection is invisible to the agent.
    Off,
    /// Listing and reading only; nothing that changes the server.
    ReadOnly,
    /// Anything, but every non-read request needs the user's confirmation.
    Ask,
    /// Anything, never asks.
    Full,
}

/// What an agent request would do to the server (or the local disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationClass {
    /// Listing directories, reading files, reading the terminal.
    Read,
    /// Typing into a shell: running commands, sending keys.
    Exec,
    /// Creating or replacing data: uploads, downloads, writes, mkdir, rename.
    Write,
    /// Removing data or changing permissions.
    Destructive,
}

/// The outcome of checking one request against the policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Allowed only after the user says yes.
    Confirm,
    Deny,
}

/// Resolve a connection's effective level.
///
/// `ancestors` lists the enclosing folders' own settings from the innermost
/// folder outwards; `None` means "inherit".
pub fn effective_level<I>(own: Option<AccessLevel>, ancestors: I, full_for_all: bool) -> AccessLevel
where
    I: IntoIterator<Item = Option<AccessLevel>>,
{
    if full_for_all {
        return AccessLevel::Full;
    }
    own.or_else(|| ancestors.into_iter().flatten().next())
        .unwrap_or(AccessLevel::Off)
}

/// Decide one request. `granted` is true while the user's temporary
/// "allow for a while" grant for this connection is in effect.
pub fn decide(level: AccessLevel, class: OperationClass, granted: bool) -> Decision {
    match (level, class) {
        (AccessLevel::Off, _) => Decision::Deny,
        (_, OperationClass::Read) => Decision::Allow,
        (AccessLevel::ReadOnly, _) => Decision::Deny,
        (AccessLevel::Ask, _) if granted => Decision::Allow,
        (AccessLevel::Ask, _) => Decision::Confirm,
        (AccessLevel::Full, _) => Decision::Allow,
    }
}

/// Whether the agent may see the connection at all.
pub fn is_visible(level: AccessLevel) -> bool {
    level != AccessLevel::Off
}

/// Whether the agent may add connections to the vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreateMode {
    Off,
    /// Each new connection needs the user's confirmation.
    Ask,
    Allowed,
}

/// Decide an "add a connection" request. Full access for everything implies
/// adding without asking.
pub fn decide_create(mode: CreateMode, full_for_all: bool, granted: bool) -> Decision {
    match mode {
        _ if full_for_all => Decision::Allow,
        CreateMode::Off => Decision::Deny,
        CreateMode::Allowed => Decision::Allow,
        CreateMode::Ask if granted => Decision::Allow,
        CreateMode::Ask => Decision::Confirm,
    }
}

/// Decide adding one particular connection. Beyond [`decide_create`]: even
/// where adding is allowed without asking, the user confirms a connection
/// that would hand the agent more than they granted by hand — `Full` access
/// inherited from the folder the agent chose, or a route through a bastion
/// where the agent may run commands only with confirmation (`Ask`).
pub fn decide_new_connection(
    mode: CreateMode,
    full_for_all: bool,
    granted: bool,
    new_level: AccessLevel,
    jump_level: Option<AccessLevel>,
) -> Decision {
    let decision = decide_create(mode, full_for_all, granted);
    let widens = new_level == AccessLevel::Full || jump_level == Some(AccessLevel::Ask);
    if decision == Decision::Allow && widens && !full_for_all && !granted {
        Decision::Confirm
    } else {
        decision
    }
}

/// The own level a connection the agent adds is stored with. It inherits
/// its folder's level (`None`) — the agent never picks its own access — but
/// where that would hide it from the agent that just created it, it starts
/// at `Ask`, so every change on it still needs the user.
pub fn created_connection_level(inherited: AccessLevel) -> Option<AccessLevel> {
    (inherited == AccessLevel::Off).then_some(AccessLevel::Ask)
}

/// Whether a new connection may route through a jump host the agent sees at
/// `level`. Tunnelling through a bastion reaches whatever the bastion
/// reaches, so it takes the same right as running commands there.
pub fn may_use_as_jump_host(level: AccessLevel) -> bool {
    matches!(level, AccessLevel::Ask | AccessLevel::Full)
}
