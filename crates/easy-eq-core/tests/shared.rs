use easy_eq_core::{Band, Coefficients, Engine, FilterKind, PreparedGraph, Profile, MAX_BANDS};

fn band(id: &str, gain: f64) -> Band {
    Band { id: id.into(), kind: FilterKind::Peak, frequency: 1000.0, gain, q: 1.4, enabled: true }
}

#[test]
fn shared_eq_is_first_in_both_channels_and_ear_correction_is_independent() {
    let mut profile = Profile::flat("shared", "Shared and ears");
    profile.common.push(band("shared", -4.0));
    profile.ears.left.push(band("left", -6.0));
    let graph = PreparedGraph::compile(&profile, 48000.0).unwrap();
    let params = graph.parameters();
    assert_eq!(params.counts, [2, 1]);
    let shared = Coefficients::for_band(&profile.common[0], 48000.0).unwrap();
    for ear in 0..2 { assert_eq!(params.coefficients[ear][0].b0, shared.b0); }

    // Compare real samples against two separately processed stages, including
    // their independent state. Shared correction must precede ear correction.
    let mut shared_profile = profile.clone(); shared_profile.ears.left.clear();
    let mut ears_profile = profile.clone(); ears_profile.common.clear();
    let mut combined = Engine::new(graph);
    let mut first = Engine::new(PreparedGraph::compile(&shared_profile, 48000.0).unwrap());
    let mut second = Engine::new(PreparedGraph::compile(&ears_profile, 48000.0).unwrap());
    for index in 0..4096 {
        let sample = (std::f64::consts::TAU * 1000.0 * index as f64 / 48000.0).sin() * 0.1;
        let input = [sample, sample * 0.7];
        let actual = combined.process_frame(input);
        let expected = second.process_frame(first.process_frame(input));
        for ear in 0..2 { assert!((actual[ear] - expected[ear]).abs() < 1e-12); }
    }
    let left_db: f64 = params.coefficients[0][..2].iter().map(|c|c.magnitude_db(1000.0, 48000.0)).sum();
    let right_db = params.coefficients[1][0].magnitude_db(1000.0, 48000.0);
    assert!((left_db + 10.0).abs() < 1e-9); assert!((right_db + 4.0).abs() < 1e-9);
}

#[test]
fn legacy_profiles_remain_flat_in_the_shared_stage_and_exports_keep_the_order() {
    let profile = Profile::flat("legacy", "Legacy");
    let mut legacy = serde_json::to_value(profile).unwrap();
    legacy.as_object_mut().unwrap().remove("common");
    let mut restored = easy_eq_core::parse_profile(&legacy.to_string()).unwrap();
    assert!(restored.common.is_empty());
    restored.common.push(band("common", -4.0)); restored.ears.left.push(band("ear", -6.0));
    let output = easy_eq_core::export::apo(&restored, true, 48000.0).unwrap();
    assert!(output.find("Gain -4.00").unwrap() < output.find("Gain -6.00").unwrap());
    assert_eq!(easy_eq_core::parse_profile(&serde_json::to_string(&restored).unwrap()).unwrap(), restored);
}

#[test]
fn combined_capacity_is_rejected_before_compilation() {
    let mut profile = Profile::flat("capacity", "Capacity");
    for index in 0..MAX_BANDS { profile.ears.left.push(band(&format!("ear-{index}"), 0.0)); }
    profile.validate().unwrap();
    profile.common.push(band("shared", 0.0));
    assert!(profile.validate().is_err());
    assert!(PreparedGraph::compile(&profile, 48000.0).is_err());
}
