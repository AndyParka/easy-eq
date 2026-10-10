use crate::editor;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use easy_eq_core::{Coefficients, Profile};

pub const LEFT: Color32 = Color32::from_rgb(52, 137, 221);
pub const RIGHT: Color32 = Color32::from_rgb(225, 137, 57);
pub const COMMON: Color32 = Color32::from_rgb(174, 129, 223);
const TEXT: Color32 = Color32::from_rgb(157, 173, 191);

fn position(rect: Rect, frequency: f64, gain: f64) -> Pos2 {
    Pos2::new(rect.left() + ((frequency / 10.0).ln() / 2000f64.ln()) as f32 * rect.width(),
        rect.center().y - (gain as f32 / 24.0) * rect.height() * 0.5)
}

fn values(rect: Rect, pointer: Pos2) -> (f64, f64) {
    let frequency = 10.0 * 2000f64.powf(((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64);
    let gain = ((rect.center().y - pointer.y) / (rect.height() * 0.5) * 24.0).clamp(-24.0, 24.0) as f64;
    (frequency, gain)
}

pub fn show(ui: &mut egui::Ui, profile: &mut Profile, ear: usize, overlay: bool,
    selected: &mut Option<String>, linked: bool, rate: f64) -> bool {
    let (response, painter) = ui.allocate_painter(Vec2::new(ui.available_width(), 224.0), Sense::click_and_drag());
    let plot = response.rect.shrink2(Vec2::new(34.0, 25.0));
    let mut changed = false;
    painter.rect_filled(response.rect, 5.0, Color32::from_rgb(24, 32, 43));
    for db in [-24, -12, 0, 12, 24] {
        let y = position(plot, 10.0, db as f64).y;
        painter.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(if db == 0 { 1.2 } else { 0.6 }, Color32::from_gray(if db == 0 { 100 } else { 57 })));
        painter.text(Pos2::new(response.rect.left() + 5.0, y), Align2::LEFT_CENTER,
            format!("{db:+}"), FontId::proportional(10.5), TEXT);
    }
    for frequency in [20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0] {
        let x = position(plot, frequency, 0.0).x;
        painter.line_segment([Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())], Stroke::new(0.6, Color32::from_gray(57)));
        let label = if frequency >= 1000.0 { format!("{}k", frequency / 1000.0) } else { format!("{frequency:.0}") };
        painter.text(Pos2::new(x, response.rect.bottom() - 7.0), Align2::CENTER_BOTTOM, label, FontId::proportional(10.5), TEXT);
    }
    let clipped = painter.with_clip_rect(plot.expand(1.0));
    for side in if ear < 2 { [1 - ear, ear] } else { [0, 1] } {
        if !overlay && side != ear { continue; }
        let coefficients: Vec<_> = profile.common.iter().chain(editor::bands(profile, side)).filter(|b| b.enabled)
            .filter_map(|b| Coefficients::for_band(b, rate).ok()).collect();
        let points: Vec<_> = (0..=360).map(|i| {
            let frequency = 10.0 * 2000f64.powf(i as f64 / 360.0);
            let db = coefficients.iter().map(|c| c.magnitude_db(frequency.min(rate * 0.49), rate)).sum::<f64>();
            position(plot, frequency, db.clamp(-24.0, 24.0))
        }).collect();
        clipped.add(egui::Shape::line(points, Stroke::new(if side == ear { 2.2 } else { 1.4 }, if side == 0 { LEFT } else { RIGHT })));
    }

    if ear == 2 {
        let coefficients: Vec<_> = profile.common.iter().filter(|b| b.enabled).filter_map(|b| Coefficients::for_band(b, rate).ok()).collect();
        let points = (0..=360).map(|i| {
            let f = 10.0 * 2000f64.powf(i as f64 / 360.0);
            let db = coefficients.iter().map(|c|c.magnitude_db(f.min(rate * 0.49), rate)).sum::<f64>();
            position(plot, f, db.clamp(-24.0, 24.0))
        }).collect();
        clipped.add(egui::Shape::line(points, Stroke::new(2.2, COMMON)));
    }

    let color = match ear { 0 => LEFT, 1 => RIGHT, _ => COMMON };
    for (index, band) in editor::bands(profile, ear).iter().enumerate() {
        let point = position(plot, band.frequency, band.gain);
        let chosen = selected.as_ref() == Some(&band.id);
        if chosen { painter.circle_stroke(point, 9.0, Stroke::new(1.0, color.gamma_multiply(0.7))); }
        if band.enabled { painter.circle_filled(point, if chosen { 5.0 } else { 4.0 }, color); }
        else { painter.circle_stroke(point, 4.0, Stroke::new(1.2, TEXT)); }
        if chosen {
            painter.text(point + Vec2::new(0.0, -13.0), Align2::CENTER_BOTTOM, format!("{}", index + 1), FontId::proportional(11.0), Color32::WHITE);
        }
    }

    if response.drag_started() || response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            *selected = editor::bands(profile, ear).iter().min_by(|a, b|
                position(plot, a.frequency, a.gain).distance_sq(pointer).total_cmp(&position(plot, b.frequency, b.gain).distance_sq(pointer)))
                .filter(|b| position(plot, b.frequency, b.gain).distance(pointer) < 18.0).map(|b| b.id.clone());
        }
    }

    let index = editor::bands(profile, ear).iter().position(|b| Some(&b.id) == selected.as_ref());
    if let Some(index) = index {
        let band = &mut editor::bands_mut(profile, ear)[index];
        if response.dragged() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let (frequency, gain) = values(plot, pointer);
                band.frequency = frequency; band.gain = gain; changed = true;
            }
        }
        if response.double_clicked() { band.gain = 0.0; changed = true; }
        if response.hovered() && !ui.ctx().input(|i| i.pointer.any_down()) {
            let delta = ui.ctx().input(|i| i.smooth_scroll_delta.y);
            if delta != 0.0 {
                band.q = (band.q * (delta as f64 * 0.006).exp()).clamp(0.1, 30.0);
                ui.ctx().input_mut(|i| { i.smooth_scroll_delta = Vec2::ZERO; });
                changed = true;
            }
        }
        if changed && linked { editor::mirror_band(profile, ear, index); }
    }
    response.on_hover_text("Drag a node to set frequency and gain. Scroll to change its Q. Double-click a node to reset gain.");
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graph_roundtrips_the_full_editable_frequency_and_gain_ranges() {
        let rect = Rect::from_min_size(Pos2::new(30.0, 20.0), Vec2::new(900.0, 200.0));
        for frequency in [10.0, 31.0, 1000.0, 16000.0, 20000.0] {
            for gain in [-24.0, -7.5, 0.0, 12.0, 24.0] {
                let (actual_f, actual_g) = values(rect, position(rect, frequency, gain));
                assert!((actual_f / frequency - 1.0).abs() < 0.00001);
                assert!((actual_g - gain).abs() < 0.00001);
            }
        }
    }
}

