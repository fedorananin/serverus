#![forbid(unsafe_code)]

//! Infrastructure implementations of Serverus application ports.

use serverus_application::context::RuntimeContextIdGenerator;
use serverus_domain::runtime_context::RuntimeContextId;

/// Generates opaque process-generation identifiers from random UUIDs.
#[derive(Clone, Copy, Debug, Default)]
pub struct UuidRuntimeContextIdGenerator;

impl RuntimeContextIdGenerator for UuidRuntimeContextIdGenerator {
    fn next_id(&self) -> RuntimeContextId {
        loop {
            if let Ok(id) = RuntimeContextId::try_from(uuid::Uuid::new_v4().as_u128()) {
                return id;
            }
        }
    }
}
