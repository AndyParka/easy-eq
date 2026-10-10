use easy_eq_core::{Band, Coefficients, Engine, FilterKind, LeaseSnapshot, LeasedProcessor, PanMode, PreparedGraph, Profile, parse_profile, export};

fn band(kind: FilterKind, frequency: f64, gain: f64, q: f64) -> Band {
    Band { id: "test-band".into(), kind, frequency, gain, q, enabled: true }
}
#[test]
fn flat_is_identity_on_the_current_frame() {
    let graph = PreparedGraph::compile(&Profile::flat("flat", "Flat"), 48000.0).unwrap();
    let mut engine = Engine::new(graph);
    assert_eq!(graph.headroom_db, 0.0);
    assert_eq!(engine.process_frame([1.0, -0.25]), [1.0, -0.25]);
    assert_eq!(engine.process_frame([0.0, 0.0]), [0.0, 0.0]);
}
#[test]
fn peak_and_shelf_responses_are_correct_across_rates() {
    for rate in [22050.0, 44100.0, 48000.0, 96000.0, 192000.0, 384000.0] {
        for (kind, expected) in [(FilterKind::Peak, 9.0), (FilterKind::LowShelf, 4.5), (FilterKind::HighShelf, 4.5)] {
            let c = Coefficients::for_band(&band(kind, 2000.0, 9.0, 1.4), rate).unwrap();
            assert!((c.magnitude_db(2000.0, rate)-expected).abs() < 1e-7);
        }
    }
}
#[test]
fn manual_gain_does_not_automatically_reduce_a_boosted_profile() {
    let mut p = Profile::flat("manual", "Manual gain"); p.gain_db = -6.0;
    p.ears.right.push(band(FilterKind::Peak, 2000.0, 9.0, 3.0));
    assert!(!p.auto_headroom);
    let graph = PreparedGraph::compile(&p, 48000.0).unwrap();
    assert_eq!(graph.headroom_db, 0.0);
    let text = export::apo(&p, true, 48000.0).unwrap();
    assert_eq!(text.matches("Preamp: -6.00 dB").count(), 2);
    assert!(text.contains("Gain 9.00 dB"));
}
#[test]
fn right_correction_does_not_leak_left_samples_to_the_right() {
    let mut p = Profile::flat("ears", "Independent ears");
    p.ears.right.push(band(FilterKind::Peak, 2000.0, -9.0, 2.0));
    let mut engine = Engine::new(PreparedGraph::compile(&p, 48000.0).unwrap());
    for i in 0..48000 {
        let x = if i == 0 { 1.0 } else { 0.0 };
        let out = engine.process_frame([x, 0.0]);
        assert_eq!(out, [x, 0.0]);
    }
}
#[test]
fn balance_preserves_spatial_channel_separation() {
    let mut p = Profile::flat("balance", "Balance"); p.pan = 0.4;
    let mut engine = Engine::new(PreparedGraph::compile(&p, 48000.0).unwrap());
    assert_eq!(engine.process_frame([1.0, 0.0]), [0.6, 0.0]);
    assert_eq!(engine.process_frame([0.0, 1.0]), [0.0, 1.0]);
}
#[test]
fn headroom_is_shared_and_includes_summed_pan_and_shelf_extremes() {
    let mut p = Profile::flat("headroom", "Headroom");
    p.auto_headroom = true; // Legacy profile support; the editor offers manual gain only.
    p.ears.right.push(band(FilterKind::Peak, 2000.0, 9.0, 3.0));
    let graph = PreparedGraph::compile(&p, 48000.0).unwrap();
    assert!(graph.headroom_db <= -10.0);
    p.ears.right.clear(); p.pan_mode = PanMode::Pan; p.pan = 1.0;
    assert!(PreparedGraph::compile(&p, 48000.0).unwrap().headroom_db <= -7.0);
    p.pan_mode = PanMode::Balance; p.pan = 0.0;
    p.ears.right.push(band(FilterKind::HighShelf, 20000.0, 12.0, 0.7071));
    assert!(PreparedGraph::compile(&p, 192000.0).unwrap().headroom_db <= -13.0);
}
#[test]
fn rapid_profile_switches_finish_at_the_latest_profile_without_nan() {
    let flat = PreparedGraph::identity(48000.0).unwrap(); let mut engine = Engine::new(flat);
    let mut p = Profile::flat("switch", "Switch"); p.auto_headroom = false;
    for gain in [-6.0, -12.0, -18.0] {
        p.gain_db = gain; engine.set_prepared(PreparedGraph::compile(&p, 48000.0).unwrap()).unwrap();
        assert!(engine.process_frame([1.0, 1.0]).iter().all(|x| x.is_finite()));
    }
    for _ in 0..1000 { engine.process_frame([1.0, 1.0]); }
    let output = engine.process_frame([1.0, 1.0]);
    assert!((output[0]-10f64.powf(-18.0/20.0)).abs() < 1e-12);
}
#[test]
fn an_expired_host_lease_returns_to_identity_without_muting_playback() {
    let mut p = Profile::flat("lease", "Lease"); p.gain_db = -12.0;
    let mut processor = LeasedProcessor::new(PreparedGraph::compile(&p, 48000.0).unwrap()).unwrap();
    let mut samples = vec![1.0f32; 1024];
    processor.process_interleaved(&mut samples, LeaseSnapshot { enabled: true, expires_at_nanos: 2000 }, 1000).unwrap();
    assert!((samples[1023]-10f32.powf(-12.0/20.0)).abs() < 1e-6);
    samples.fill(1.0);
    processor.process_interleaved(&mut samples, LeaseSnapshot { enabled: true, expires_at_nanos: 2000 }, 2000).unwrap();
    assert_eq!(samples[1023], 1.0);
    assert!(samples.iter().all(|x| x.is_finite() && *x > 0.0));
}
#[test]
fn renderer_absence_does_not_disable_a_live_audio_host() {
    let mut p = Profile::flat("host", "Host"); p.gain_db = -6.0;
    let mut processor = LeasedProcessor::new(PreparedGraph::compile(&p, 48000.0).unwrap()).unwrap();
    let mut samples = vec![1.0f32; 1024];
    // No UI/renderer is constructed or consulted; the host owns this lease.
    processor.process_interleaved(&mut samples, LeaseSnapshot { enabled: true, expires_at_nanos: u64::MAX }, 5000).unwrap();
    assert!((samples[1023]-10f32.powf(-6.0/20.0)).abs() < 1e-6);
}
#[test]
fn malformed_profiles_fail_before_audio_state_changes() {
    let mut p = Profile::flat("invalid", "Invalid"); p.pan = f64::NAN;
    assert!(PreparedGraph::compile(&p, 48000.0).is_err());
    p.pan = 0.0; p.ears.left = vec![band(FilterKind::Peak, 1000.0, 0.0, 1.0); 32];
    assert!(p.validate().is_err());
    assert!(parse_profile("{}").is_err());
    assert!(PreparedGraph::identity(7999.0).is_err());
}
#[test]
fn voice_and_high_rate_outputs_accept_finite_transport_and_reject_unstable_coefficients() {
    let mut p=Profile::flat("rates","Rates");p.gain_db=-3.0;
    for (index,kind) in [FilterKind::Peak,FilterKind::LowShelf,FilterKind::HighShelf].into_iter().enumerate() {
        let mut b=band(kind,1000.0,-6.0,1.1);b.id=format!("rate-{index}");p.ears.left.push(b);
    }
    for rate in [8000.0,16000.0,48000.0,768000.0] {
        let graph=PreparedGraph::compile(&p,rate).unwrap();
        let parameters=graph.parameters();assert!(PreparedGraph::from_parameters(parameters).is_some());
        let mut unstable=parameters;unstable.coefficients[0][0].a2=1.5;assert!(PreparedGraph::from_parameters(unstable).is_none());
        let mut invalid=parameters;invalid.counts[0]=32;assert!(PreparedGraph::from_parameters(invalid).is_none());
    }
}
#[test]
fn apo_export_bypasses_and_scopes_the_correct_ears() {
    let mut p = Profile::flat("export", "Export"); p.ears.left.push(band(FilterKind::LowShelf, 60.0, -3.0, 0.7071));
    let text = export::apo(&p, true, 48000.0).unwrap();
    assert!(text.contains("Channel: L\nPreamp: 0.00 dB\nFilter 1: ON LSC"));
    assert!(text.ends_with("Channel: ALL\n"));
    let bypass = export::apo(&p, false, 48000.0).unwrap();
    assert!(!bypass.contains("Filter 1:") && !bypass.contains("Preamp:"));
}
