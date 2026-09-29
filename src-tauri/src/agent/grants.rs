//! Temporary "allow for a while" grants the user gives from a confirmation
//! dialog. A grant belongs to one connection and one vault unlock: locking
//! the vault (a new access epoch after unlock) silently voids it.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long "Allow for 15 minutes" lasts.
pub const GRANT_DURATION: Duration = Duration::from_secs(15 * 60);

struct Grant {
    epoch: String,
    until: Instant,
}

#[derive(Default)]
pub struct Grants {
    by_connection: Mutex<HashMap<String, Grant>>,
}

impl Grants {
    pub fn grant(&self, connection_id: &str, epoch: &str, now: Instant) {
        self.by_connection.lock().unwrap().insert(
            connection_id.to_string(),
            Grant {
                epoch: epoch.to_string(),
                until: now + GRANT_DURATION,
            },
        );
    }

    pub fn is_granted(&self, connection_id: &str, epoch: &str, now: Instant) -> bool {
        let mut grants = self.by_connection.lock().unwrap();
        grants.retain(|_, grant| grant.until > now);
        grants
            .get(connection_id)
            .is_some_and(|grant| grant.epoch == epoch)
    }
}
