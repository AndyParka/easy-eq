use easy_eq_core::{Band, Coefficients, PreparedGraph, Profile};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case { rate: f64, band: Band, coefficients: Coefficients }
#[derive(Deserialize)]
struct Reference { coefficients: Vec<Case>, profile: Profile, headroom: f64 }

#[test]
fn native_coefficients_and_headroom_match_the_independent_editor_reference() {
    let fixture: Reference = serde_json::from_str(include_str!("reference.json")).unwrap();
    assert_eq!(fixture.coefficients.len(), 54);
    for case in fixture.coefficients {
        let native = Coefficients::for_band(&case.band, case.rate).unwrap();
        for (actual, expected) in [(native.b0, case.coefficients.b0), (native.b1, case.coefficients.b1),
            (native.b2, case.coefficients.b2), (native.a1, case.coefficients.a1), (native.a2, case.coefficients.a2)] {
            assert!((actual-expected).abs() < 1e-12, "{:?} at {} Hz: {actual} != {expected}", case.band.kind, case.rate);
        }
    }
    assert_eq!(PreparedGraph::compile(&fixture.profile, 48000.0).unwrap().headroom_db, fixture.headroom);
}
