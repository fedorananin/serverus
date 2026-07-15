use super::desktop_quick_unlock;

#[test]
fn scenario_build_always_disables_platform_quick_unlock() {
    let quick_unlock = desktop_quick_unlock();

    assert_eq!(quick_unlock.method_name(), "Biometric unlock");
    assert!(!quick_unlock.is_available());
}
