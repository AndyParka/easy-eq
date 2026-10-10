use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_BANDS: usize = 31;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum FilterKind { Peak, LowShelf, HighShelf }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PanMode { Balance, Pan }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Band {
    pub id: String,
    pub kind: FilterKind,
    pub frequency: f64,
    pub gain: f64,
    pub q: f64,
    pub enabled: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ears { pub left: Vec<Band>, pub right: Vec<Band> }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub version: u32,
    pub id: String,
    pub name: String,
    /// Shared EQ runs before each ear's correction. Missing in legacy profiles.
    #[serde(default)]
    pub common: Vec<Band>,
    pub ears: Ears,
    pub gain_db: f64,
    pub pan: f64,
    pub pan_mode: PanMode,
    pub auto_headroom: bool,
}
pub fn finite(value: f64, min: f64, max: f64, name: &str) -> Result<(), Error> {
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(Error(format!("{name} must be finite and between {min} and {max}")));
    }
    Ok(())
}
fn identity(value: &str, limit: usize, name: &str) -> Result<(), Error> {
    if value.trim().is_empty() || value.chars().count() > limit {
        return Err(Error(format!("Invalid {name}")));
    }
    Ok(())
}
impl Band {
    pub fn validate(&self) -> Result<(), Error> {
        identity(&self.id, 100, "band ID")?;
        finite(self.frequency, 10.0, 20000.0, "Frequency")?;
        finite(self.gain, -24.0, 24.0, "Band gain")?;
        finite(self.q, 0.1, 30.0, "Q")
    }
}
impl Profile {
    pub fn validate(&self) -> Result<(), Error> {
        if self.version != 1 { return Err(Error("Unsupported profile version".into())); }
        identity(&self.id, 100, "profile ID")?;
        identity(&self.name, 80, "profile name")?;
        finite(self.gain_db, -60.0, 0.0, "Output gain")?;
        finite(self.pan, -1.0, 1.0, "Pan")?;
        for ear in [&self.ears.left, &self.ears.right] {
            if self.common.len() + ear.len() > MAX_BANDS {
                return Err(Error("Shared and per-ear filters must total at most 31 bands per channel".into()));
            }
        }
        for bands in [&self.common, &self.ears.left, &self.ears.right] {
            if bands.len() > MAX_BANDS { return Err(Error("Too many bands in one ear".into())); }
            let mut ids = HashSet::new();
            for band in bands {
                band.validate()?;
                if !ids.insert(&band.id) { return Err(Error("Duplicate band ID within one ear".into())); }
            }
        }
        Ok(())
    }
    pub fn flat(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { version: 1, id: id.into(), name: name.into(), common: vec![],
            ears: Ears { left: vec![], right: vec![] }, gain_db: 0.0, pan: 0.0,
            pan_mode: PanMode::Balance, auto_headroom: false }
    }
    /// Matrix order LL, LR, RL, RR. Balance does not cross-mix spatial channels.
    pub fn matrix(&self) -> [f64; 4] {
        let p = self.pan;
        match self.pan_mode {
            PanMode::Balance => [if p > 0.0 { 1.0-p } else { 1.0 }, 0.0, 0.0,
                if p < 0.0 { 1.0+p } else { 1.0 }],
            PanMode::Pan if p < 0.0 => [1.0, -p, 0.0, 1.0+p],
            PanMode::Pan => [1.0-p, 0.0, p, 1.0],
        }
    }
}
