use crate::agent::hub::unstable_location;

#[test]
fn temporary_app_locations_are_flagged() {
    let translocated =
        "/private/var/folders/x/T/AppTranslocation/1234/d/Serverus.app/Contents/MacOS/serverus";
    assert!(unstable_location(translocated).is_some());
    assert!(unstable_location("/Volumes/Serverus/Serverus.app/Contents/MacOS/serverus").is_some());
    // Installed apps, including on another disk, are fine.
    assert!(unstable_location("/Applications/Serverus.app/Contents/MacOS/serverus").is_none());
    assert!(
        unstable_location("/Volumes/Data/Applications/Serverus.app/Contents/MacOS/serverus")
            .is_none()
    );
    assert!(unstable_location("/home/me/.local/bin/serverus").is_none());
}
