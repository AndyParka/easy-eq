use easy_eq_core::{Band, FilterKind, Profile, MAX_BANDS};

pub fn bands(profile: &Profile, ear: usize) -> &[Band] {
    match ear { 0 => &profile.ears.left, 1 => &profile.ears.right, _ => &profile.common }
}

pub fn bands_mut(profile: &mut Profile, ear: usize) -> &mut Vec<Band> {
    match ear { 0 => &mut profile.ears.left, 1 => &mut profile.ears.right, _ => &mut profile.common }
}

pub fn capacity(profile: &Profile, ear: usize, linked: bool) -> usize {
    if ear == 2 { MAX_BANDS - profile.ears.left.len().max(profile.ears.right.len()) }
    else if linked { MAX_BANDS - profile.common.len() - profile.ears.left.len().max(profile.ears.right.len()) + bands(profile, ear).len() }
    else { MAX_BANDS - profile.common.len() }
}

// Imported profiles can use different IDs for corresponding left/right bands.
// Match an existing ID first, then the band position, retaining the target ID.
pub fn mirror_band(profile: &mut Profile, ear: usize, index: usize) {
    if ear == 2 { return; }
    let Some(patch) = bands(profile, ear).get(index).cloned() else { return; };
    let limit = MAX_BANDS - profile.common.len();
    let other = bands_mut(profile, 1 - ear);
    if let Some(target) = other.iter_mut().find(|band| band.id == patch.id) {
        *target = patch;
    } else if let Some(target) = other.get_mut(index) {
        let id = target.id.clone();
        *target = Band { id, ..patch };
    } else if other.len() < limit {
        other.push(patch);
    }
}

pub fn add_band(profile: &mut Profile, ear: usize, linked: bool, band: Band) -> bool {
    let linked = linked && ear < 2;
    if bands(profile, ear).len() >= capacity(profile, ear, linked) { return false; }
    bands_mut(profile, ear).push(band.clone());
    if linked { bands_mut(profile, 1 - ear).push(band); }
    true
}

pub fn remove_band(profile: &mut Profile, ear: usize, linked: bool, index: usize) {
    if ear == 2 { if index < profile.common.len() { profile.common.remove(index); } return; }
    let Some(band) = bands(profile, ear).get(index) else { return; };
    let other_index = bands(profile, 1 - ear).iter().position(|b| b.id == band.id).unwrap_or(index);
    bands_mut(profile, ear).remove(index);
    if linked && other_index < bands(profile, 1 - ear).len() {
        bands_mut(profile, 1 - ear).remove(other_index);
    }
}

pub fn reset_ear(profile: &mut Profile, ear: usize, linked: bool) {
    if ear == 2 { for band in &mut profile.common { band.gain = 0.0; band.enabled = true; } return; }
    for side in 0..2 {
        if side == ear || linked {
            for band in bands_mut(profile, side) { band.gain = 0.0; band.enabled = true; }
        }
    }
}

pub fn copy_ear(profile: &mut Profile, ear: usize) {
    if ear == 2 { return; }
    let source = bands(profile, ear).to_vec();
    *bands_mut(profile, 1 - ear) = source;
}

pub fn shape_name(kind: FilterKind) -> &'static str {
    match kind { FilterKind::Peak => "Bell", FilterKind::LowShelf => "Low shelf", FilterKind::HighShelf => "High shelf" }
}

#[derive(Default)]
pub struct History {
    profile_id: String,
    undo: Vec<Profile>,
    redo: Vec<Profile>,
    gesture: bool,
}

impl History {
    pub fn select(&mut self, id: &str) {
        if self.profile_id != id { *self = Self { profile_id: id.into(), ..Self::default() }; }
    }
    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }
    pub fn end_gesture(&mut self) { self.gesture = false; }
    pub fn record(&mut self, before: &Profile, after: &Profile, pointer_down: bool) {
        self.select(&before.id);
        if before == after { if !pointer_down { self.end_gesture(); } return; }
        if !self.gesture {
            if self.undo.len() == 64 { self.undo.remove(0); }
            self.undo.push(before.clone());
        }
        self.redo.clear();
        self.gesture = pointer_down;
    }
    pub fn undo(&mut self, current: &mut Profile) -> bool {
        self.select(&current.id);
        if let Some(previous) = self.undo.pop() {
            self.redo.push(std::mem::replace(current, previous)); self.end_gesture(); true
        } else { false }
    }
    pub fn redo(&mut self, current: &mut Profile) -> bool {
        self.select(&current.id);
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(current, next)); self.end_gesture(); true
        } else { false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> Profile {
        let mut p=Profile::flat("test", "Test");
        p.ears.left=crate::controller::flat_bands();p.ears.right=p.ears.left.clone();p
    }

    #[test]
    fn linking_imported_ears_preserves_target_ids_and_all_parameters() {
        let mut p = profile();
        p.ears.right[0].id = "imported-right".into();
        p.ears.left[0].kind = FilterKind::LowShelf;
        p.ears.left[0].gain = -7.0;
        p.ears.left[0].enabled = false;
        mirror_band(&mut p, 0, 0);
        assert_eq!(p.ears.right[0].id, "imported-right");
        assert_eq!(p.ears.right[0].kind, FilterKind::LowShelf);
        assert_eq!(p.ears.right[0].gain, -7.0);
        assert!(!p.ears.right[0].enabled);
        remove_band(&mut p, 0, true, 0);
        assert_eq!(p.ears.left.len(), 12); assert_eq!(p.ears.right.len(), 12);
        p.validate().unwrap();
    }

    #[test]
    fn independent_edit_and_reset_leave_the_other_ear_and_output_gain_alone() {
        let mut p = profile(); p.gain_db = -9.0;
        p.ears.left[0].gain = 4.0; p.ears.right[0].gain = -8.0;
        reset_ear(&mut p, 0, false);
        assert_eq!(p.ears.left[0].gain, 0.0); assert_eq!(p.ears.right[0].gain, -8.0);
        assert_eq!(p.gain_db, -9.0);
        copy_ear(&mut p, 1); assert_eq!(p.ears.left, p.ears.right);
    }

    #[test]
    fn shared_controls_do_not_copy_or_erase_individual_ear_correction() {
        let mut p = profile(); let original_left = p.ears.left.clone(); let original_right = p.ears.right.clone();
        let band = Band { id: "shared".into(), kind: FilterKind::LowShelf, frequency: 80.0, gain: -4.0, q: 0.707, enabled: true };
        assert!(add_band(&mut p, 2, true, band));
        mirror_band(&mut p, 2, 0); reset_ear(&mut p, 2, true);
        assert_eq!(p.common[0].gain, 0.0);
        assert_eq!(p.ears.left, original_left); assert_eq!(p.ears.right, original_right);
        remove_band(&mut p, 2, true, 0); assert!(p.common.is_empty());
        p.validate().unwrap();
    }

    #[test]
    fn a_drag_is_one_undo_step_and_redo_cannot_cross_profiles() {
        let mut history = History::default(); let mut p = profile(); let original = p.clone();
        for gain in [1.0, 2.0, 3.0] {
            let before = p.clone(); p.ears.left[0].gain = gain;
            history.record(&before, &p, true);
        }
        history.end_gesture(); let edited = p.clone();
        assert!(history.undo(&mut p)); assert_eq!(p, original); assert!(!history.can_undo());
        assert!(history.redo(&mut p)); assert_eq!(p, edited);
        history.select("another-profile"); assert!(!history.can_undo()); assert!(!history.can_redo());
    }
}

