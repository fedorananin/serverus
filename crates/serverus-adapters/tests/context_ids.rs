use std::collections::HashSet;

use serverus_adapters::UuidRuntimeContextIdGenerator;
use serverus_application::context::RuntimeContextIdGenerator;

#[test]
fn generated_context_ids_are_non_zero_and_unique() {
    let generator = UuidRuntimeContextIdGenerator;

    let ids = (0..128)
        .map(|_| generator.next_id().get())
        .collect::<HashSet<_>>();

    assert_eq!(ids.len(), 128);
    assert!(!ids.contains(&0));
}
