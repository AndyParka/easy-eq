//! Shared native audio core. Audio callbacks do not depend on a renderer or webview.
pub mod dsp;
pub mod model;
pub mod export;

pub use dsp::{Coefficients, Engine, GraphParameters, LeaseSnapshot, LeasedProcessor, PreparedGraph};
pub use model::{Band, Ears, FilterKind, PanMode, Profile, MAX_BANDS};

#[derive(Debug, Clone, PartialEq)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.fmt(f) }
}
impl std::error::Error for Error {}

pub fn parse_profile(json: &str) -> Result<Profile, Error> {
    if json.len() > 1024 * 1024 { return Err(Error("Profile exceeds 1 MB".into())); }
    let profile: Profile = serde_json::from_str(json).map_err(|e| Error(e.to_string()))?;
    profile.validate()?;
    Ok(profile)
}
pub mod workspace;
