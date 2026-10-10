use crate::{Error, FilterKind, PreparedGraph, Profile};
use std::fmt::Write;

/// Configuration generation happens outside the real-time callback.
pub fn apo(profile: &Profile, enabled: bool, rate: f64) -> Result<String, Error> {
    let graph = PreparedGraph::compile(profile, rate)?;
    let mut text = String::from("# Easy EQ | minimum-phase EQ | no added processing buffer\n# Include only for your selected stereo output.\n");
    if !enabled { text.push_str("# Bypassed: identity processing.\nChannel: ALL\n"); return Ok(text); }
    let matrix = profile.matrix();
    writeln!(text, "Channel: L R\nCopy: L={:.8}*L+{:.8}*R R={:.8}*L+{:.8}*R", matrix[0], matrix[1], matrix[2], matrix[3]).unwrap();
    if !profile.common.is_empty() {
        text.push_str("# Shared EQ before per-ear correction\nChannel: L R\n");
        for (i, b) in profile.common.iter().filter(|b| b.enabled).enumerate() {
            let kind = match b.kind { FilterKind::Peak => "PK", FilterKind::LowShelf => "LSC", FilterKind::HighShelf => "HSC" };
            writeln!(text, "Filter {}: ON {kind} Fc {:.2} Hz Gain {:.2} dB Q {:.4}", i+1, b.frequency.min(rate*0.49), b.gain, b.q).unwrap();
        }
    }
    for (ear, bands) in [("L", &profile.ears.left), ("R", &profile.ears.right)] {
        writeln!(text, "Channel: {ear}\nPreamp: {:.2} dB", profile.gain_db+graph.headroom_db).unwrap();
        for (i, b) in bands.iter().filter(|b| b.enabled).enumerate() {
            let kind = match b.kind { FilterKind::Peak => "PK", FilterKind::LowShelf => "LSC", FilterKind::HighShelf => "HSC" };
            writeln!(text, "Filter {}: ON {kind} Fc {:.2} Hz Gain {:.2} dB Q {:.4}", i+1, b.frequency.min(rate*0.49), b.gain, b.q).unwrap();
        }
    }
    text.push_str("Channel: ALL\n"); Ok(text)
}
