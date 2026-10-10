use crate::{Error, Profile};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ProfileMode { #[default] PerDevice, Global }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub profile_mode: ProfileMode,
    pub start_at_login: bool,
    pub device_profiles: BTreeMap<String, String>,
    pub global_profile_id: Option<String>,
    pub last_output_id: Option<String>,
}
impl Default for Settings {
    fn default() -> Self { Self { profile_mode: ProfileMode::PerDevice, start_at_login: true,
        device_profiles: BTreeMap::new(), global_profile_id: None, last_output_id: None } }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub profiles: Vec<Profile>, pub active_id: String, pub enabled: bool, pub view: String,
    #[serde(default)] pub settings: Settings,
    #[serde(default)] pub revision: u64,
}
impl Workspace {
    pub fn parse(json: &str) -> Result<Self, Error> {
        if json.len() > 4 * 1024 * 1024 { return Err(Error("Workspace exceeds 4 MB".into())); }
        let mut workspace: Self = serde_json::from_str(json).map_err(|e| Error(e.to_string()))?;
        for profile in &mut workspace.profiles { profile.auto_headroom = false; profile.pan_mode = crate::PanMode::Balance; }
        workspace.validate()?; Ok(workspace)
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.profiles.is_empty() || self.profiles.len() > 100 || !["overlay", "focus"].contains(&self.view.as_str()) {
            return Err(Error("Invalid workspace".into()));
        }
        let mut ids = HashSet::new();
        for profile in &self.profiles { profile.validate()?; if !ids.insert(&profile.id) { return Err(Error("Duplicate profile ID".into())); } }
        if !ids.contains(&self.active_id) || self.settings.global_profile_id.as_ref().is_some_and(|id| !ids.contains(id)) {
            return Err(Error("Missing active or shared profile".into()));
        }
        if self.settings.device_profiles.len() > 1024 || self.settings.device_profiles.iter().any(|(output,id)| output.is_empty() || output.len() > 1024 || !ids.contains(id)) {
            return Err(Error("Invalid output/profile binding".into()));
        }
        if self.settings.last_output_id.as_ref().is_some_and(|id| id.is_empty() || id.len() > 1024) { return Err(Error("Invalid output ID".into())); }
        Ok(())
    }
    /// Resolve by endpoint rather than the previous editor selection. Unassigned
    /// outputs bypass until a profile is chosen, including at the profile cap.
    pub fn profile_for_output(&self, output: &str) -> Option<&Profile> {
        let id = match self.settings.profile_mode {
            ProfileMode::Global => self.settings.global_profile_id.as_ref().unwrap_or(&self.active_id),
            ProfileMode::PerDevice => self.settings.device_profiles.get(output)?,
        };
        self.profiles.iter().find(|p| &p.id == id)
    }
    /// Host-owned routing. A renderer need not be running to select the right profile.
    pub fn follow_output(&mut self, output: Option<(&str, &str)>) -> Result<bool, Error> {
        let Some((id, name)) = output else { return Ok(false); };
        let previous = (self.active_id.clone(), self.settings.clone());
        if self.settings.profile_mode == ProfileMode::Global {
            self.active_id = self.settings.global_profile_id.clone().unwrap_or_else(|| self.active_id.clone());
            self.settings.global_profile_id = Some(self.active_id.clone());
        } else if let Some(profile_id) = self.settings.device_profiles.get(id) {
            self.active_id = profile_id.clone();
        } else {
            let profile_id = if self.settings.device_profiles.is_empty() { self.active_id.clone() } else {
                if self.profiles.len() == 100 { return Err(Error("No profile assigned to this output. Choose an existing profile; the workspace already has 100 profiles.".into())); }
                let mut index = 1;
                let profile_id = loop { let key = format!("output-profile-{index}"); if !self.profiles.iter().any(|p| p.id == key) { break key; } index += 1; };
                let name: String = name.chars().take(80).collect();
                let name = if name.trim().is_empty() { "New output".into() } else { name };
                self.profiles.push(Profile::flat(profile_id.clone(), name));
                profile_id
            };
            self.settings.device_profiles.insert(id.into(), profile_id.clone()); self.active_id = profile_id;
        }
        self.settings.last_output_id = Some(id.into());
        Ok(previous != (self.active_id.clone(), self.settings.clone()))
    }
    /// Merge renderer edits into newer host state without losing profiles created
    /// during an output switch. Only differences from the acknowledged base apply.
    pub fn merge_edits(&mut self, client: Self, base: &Self) -> Result<(), Error> {
        for old in &base.profiles {
            if !client.profiles.iter().any(|p| p.id == old.id) { self.profiles.retain(|p| p.id != old.id); }
        }
        for edited in &client.profiles {
            if base.profiles.iter().find(|p| p.id == edited.id) == Some(edited) { continue; }
            if let Some(current) = self.profiles.iter_mut().find(|p| p.id == edited.id) { *current = edited.clone(); }
            else { self.profiles.push(edited.clone()); }
        }
        for key in base.settings.device_profiles.keys() {
            if !client.settings.device_profiles.contains_key(key) { self.settings.device_profiles.remove(key); }
        }
        for (key, value) in &client.settings.device_profiles {
            if base.settings.device_profiles.get(key) != Some(value) { self.settings.device_profiles.insert(key.clone(), value.clone()); }
        }
        if client.enabled != base.enabled { self.enabled = client.enabled; }
        if client.view != base.view { self.view = client.view; }
        if client.settings.profile_mode != base.settings.profile_mode { self.settings.profile_mode = client.settings.profile_mode; }
        if client.settings.start_at_login != base.settings.start_at_login { self.settings.start_at_login = client.settings.start_at_login; }
        if client.settings.global_profile_id != base.settings.global_profile_id { self.settings.global_profile_id = client.settings.global_profile_id.clone(); }
        if client.active_id != base.active_id {
            self.active_id = client.active_id.clone();
            if client.settings.profile_mode == ProfileMode::Global { self.settings.global_profile_id = Some(client.active_id.clone()); }
            else if let Some(id) = &client.settings.last_output_id { self.settings.device_profiles.insert(id.clone(), client.active_id.clone()); }
        }
        let ids: HashSet<_> = self.profiles.iter().map(|p| p.id.clone()).collect();
        self.settings.device_profiles.retain(|_,id| ids.contains(id));
        if !ids.contains(&self.active_id) { self.active_id = self.profiles.first().ok_or_else(|| Error("Keep at least one profile".into()))?.id.clone(); }
        if self.settings.global_profile_id.as_ref().is_some_and(|id| !ids.contains(id)) { self.settings.global_profile_id = Some(self.active_id.clone()); }
        self.validate()
    }
}
