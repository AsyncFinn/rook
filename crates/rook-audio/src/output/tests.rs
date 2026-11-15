use super::chosen;
use super::watch::Changes;
use crate::Device;

#[test]
fn a_chosen_device_gives_way_to_the_default_while_unplugged() {
    let headset = Device::Named("Headset".into());
    assert_eq!(chosen(&headset, &["Speakers", "Headset"]), Some("Headset"));
    assert_eq!(chosen(&headset, &["Speakers"]), None);
    // Plugged back in, it is played through again.
    assert_eq!(chosen(&headset, &["Headset", "Speakers"]), Some("Headset"));
}

#[test]
fn the_default_device_is_the_systems_of_the_moment() {
    assert_eq!(chosen(&Device::Default, &["Speakers", "Headset"]), None);
}

#[test]
fn each_reader_takes_its_own_change() {
    let changes = Changes::default();
    changes.changed();
    assert!(changes.take_reroute());
    assert!(!changes.take_reroute(), "taken once");
    // The window still learns the list changed after the output rerouted.
    assert!(changes.take_listed());
    assert!(!changes.take_listed());
}
