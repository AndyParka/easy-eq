use easy_eq_core::{Profile, workspace::{Workspace, Settings, ProfileMode}};
fn workspace() -> Workspace {
    let mut second = Profile::flat("second", "Second"); second.gain_db = -9.0;
    Workspace { profiles: vec![Profile::flat("first", "First"), second], active_id: "first".into(), enabled: false,
        view: "overlay".into(), settings: Settings::default(), revision: 0 }
}
#[test]
fn ordered_unacknowledged_edits_can_return_to_the_original_value() {
    let mut host = workspace();
    let original = host.clone();
    let mut first = original.clone(); first.profiles[0].gain_db = -12.0;
    host.merge_edits(first.clone(), &original).unwrap();
    // The next submission uses the previous submitted state as its base.
    // Using the original acknowledged base here would lose the reversal.
    host.merge_edits(original.clone(), &first).unwrap();
    assert_eq!(host.profiles[0].gain_db, original.profiles[0].gain_db);
}
#[test] fn each_output_keeps_its_profile_and_new_outputs_start_flat() {
    let mut w = workspace(); w.follow_output(Some(("usb", "USB"))).unwrap();
    w.settings.device_profiles.insert("usb".into(), "second".into());
    w.follow_output(Some(("usb", "USB"))).unwrap(); assert_eq!(w.active_id, "second");
    w.follow_output(Some(("dock", "Dock"))).unwrap();
    assert_eq!(w.profiles.iter().find(|p| p.id == w.active_id).unwrap().gain_db, 0.0);
    w.follow_output(Some(("usb", "USB"))).unwrap(); assert_eq!(w.active_id, "second");
    assert!(!w.enabled, "Following an output must not override the saved off state");
}
#[test] fn shared_mode_follows_outputs_without_losing_individual_assignments() {
    let mut w = workspace(); w.follow_output(Some(("usb", "USB"))).unwrap();
    w.settings.profile_mode = ProfileMode::Global; w.settings.global_profile_id = Some("second".into());
    w.follow_output(Some(("dock", "Dock"))).unwrap(); assert_eq!(w.active_id, "second");
    w.follow_output(Some(("usb", "USB"))).unwrap(); assert_eq!(w.active_id, "second");
    w.settings.profile_mode = ProfileMode::PerDevice;
    w.follow_output(Some(("usb", "USB"))).unwrap(); assert_eq!(w.active_id, "first");
}
#[test] fn renderer_edits_survive_a_concurrent_native_output_switch() {
    let mut host = workspace(); host.follow_output(Some(("usb", "USB"))).unwrap();
    let base = host.clone(); let mut client = base.clone(); client.profiles[0].gain_db = -6.0;
    client.active_id = "second".into(); client.settings.device_profiles.insert("usb".into(), "second".into());
    host.follow_output(Some(("dock", "Dock"))).unwrap(); let dock_profile = host.active_id.clone();
    host.merge_edits(client, &base).unwrap(); host.follow_output(Some(("dock", "Dock"))).unwrap();
    assert_eq!(host.active_id, dock_profile);
    assert_eq!(host.profiles[0].gain_db, -6.0);
    host.follow_output(Some(("usb", "USB"))).unwrap(); assert_eq!(host.active_id, "second");
}
#[test] fn legacy_settings_migrate_and_last_on_off_state_is_preserved() {
    let mut json = serde_json::to_value(workspace()).unwrap(); json.as_object_mut().unwrap().remove("settings");
    let loaded = Workspace::parse(&json.to_string()).unwrap();
    assert_eq!(loaded.settings.profile_mode, ProfileMode::PerDevice);
    assert!(loaded.settings.start_at_login); assert!(!loaded.enabled);
    let mut invalid = loaded.clone(); invalid.settings.device_profiles.insert("usb".into(), "missing".into());
    assert!(Workspace::parse(&serde_json::to_string(&invalid).unwrap()).is_err());
}
#[test] fn profile_capacity_failure_does_not_apply_another_outputs_correction() {
    let mut w = workspace(); w.follow_output(Some(("usb", "USB"))).unwrap();
    for i in 2..100 { w.profiles.push(Profile::flat(format!("p{i}"), "Spare")); }
    let before = w.active_id.clone();
    assert!(w.follow_output(Some(("dock", "Dock"))).is_err());
    assert_eq!(w.active_id, before); assert!(!w.settings.device_profiles.contains_key("dock"));
    assert!(w.profile_for_output("dock").is_none());
}
