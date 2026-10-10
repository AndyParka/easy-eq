use crate::graph::{LEFT, RIGHT};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use easy_eq_bridge::{spectrum::{SpectrumFrame, FFT_SIZE}, windows::{endpoint_guid, now_ms, state_directory, SpectrumMapping}};
use std::time::{Duration, Instant};

const FLOOR: f32 = -96.0;
const BINS: usize = FFT_SIZE / 2 + 1;

/// Hann-windowed, one-sided FFT in dBFS. A full-scale bin-centered sine is
/// 0 dBFS. The same calibration is used for all four measured channels.
fn frequencies(samples: &[f32; FFT_SIZE]) -> [f32; BINS] {
    let mut real = [0.0f64; FFT_SIZE]; let mut imag = [0.0f64; FFT_SIZE]; let mut sum = 0.0;
    for (i, sample) in samples.iter().enumerate() {
        let window = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (FFT_SIZE - 1) as f64).cos();
        real[i] = *sample as f64 * window; sum += window;
    }
    for i in 0..FFT_SIZE {
        let j = i.reverse_bits() >> (usize::BITS - FFT_SIZE.ilog2());
        if j > i { real.swap(i, j); }
    }
    let mut length = 2;
    while length <= FFT_SIZE {
        let angle = -std::f64::consts::TAU / length as f64;
        let (step_imag, step_real) = angle.sin_cos();
        for start in (0..FFT_SIZE).step_by(length) {
            let (mut wr, mut wi) = (1.0, 0.0);
            for offset in 0..length / 2 {
                let a = start + offset; let b = a + length / 2;
                let tr = wr * real[b] - wi * imag[b]; let ti = wr * imag[b] + wi * real[b];
                real[b] = real[a] - tr; imag[b] = imag[a] - ti;
                real[a] += tr; imag[a] += ti;
                (wr, wi) = (wr * step_real - wi * step_imag, wr * step_imag + wi * step_real);
            }
        }
        length *= 2;
    }
    std::array::from_fn(|i| {
        let scale = if i == 0 || i == FFT_SIZE / 2 { 1.0 } else { 2.0 };
        (20.0 * ((real[i].hypot(imag[i]) * scale / sum).max(1e-12)).log10()) as f32
    })
}

pub struct Visualiser {
    endpoint: Option<String>, mapping: Option<SpectrumMapping>, sequence: u64,
    retry: Instant, rate: f64, captured_ms: u64, input: [[f32; BINS]; 2], output: [[f32; BINS]; 2],
}
impl Default for Visualiser {
    fn default() -> Self {
        Self { endpoint: None, mapping: None, sequence: 0, retry: Instant::now() - Duration::from_secs(2),
            rate: 48000.0, captured_ms: 0, input: [[FLOOR; BINS]; 2], output: [[FLOOR; BINS]; 2] }
    }
}
impl Visualiser {
    fn clear(&mut self) { self.captured_ms = 0; self.input.fill([FLOOR; BINS]); self.output.fill([FLOOR; BINS]); }
    pub fn poll(&mut self, output_id: Option<&str>, visible: bool) {
        if self.endpoint.as_deref() != output_id {
            self.stop(); self.mapping = None; self.endpoint = output_id.map(str::to_owned);
            self.sequence = 0; self.clear(); self.retry = Instant::now() - Duration::from_secs(2);
        }
        if !visible { self.stop(); self.clear(); return; }
        if self.mapping.is_none() && self.retry.elapsed() >= Duration::from_secs(1) {
            self.retry = Instant::now();
            self.mapping = output_id.and_then(endpoint_guid).and_then(|guid|
                SpectrumMapping::open(&state_directory().join(guid).join("spectrum.bin")).ok());
        }
        let now = now_ms();
        let frame = self.mapping.as_ref().and_then(|mapping| {
            mapping.block().request(now);
            mapping.block().read(self.sequence)
        });
        if let Some((sequence, frame)) = frame {
            self.sequence = sequence;
            if now.saturating_sub(frame.captured_ms) <= 300 {
                self.update(&frame);
            }
        }
        if now.saturating_sub(self.captured_ms) > 300 { self.clear(); }
    }
    fn update(&mut self, frame: &SpectrumFrame) {
        let fresh = self.captured_ms == 0 || self.rate != frame.rate;
        self.rate = frame.rate; self.captured_ms = frame.captured_ms;
        for ear in 0..2 {
            for (target, samples) in [(&mut self.input[ear], &frame.input[ear]), (&mut self.output[ear], &frame.output[ear])] {
                let bins = frequencies(samples);
                for (old, measured) in target.iter_mut().zip(bins) {
                    let db = measured.max(FLOOR);
                    // Fast attack and slower decay, with identical behavior in
                    // both panels so changes remain directly comparable.
                    let weight = if fresh || db > *old { 1.0 } else { 0.4 };
                    *old += (db - *old) * weight;
                }
            }
        }
    }
    pub fn stop(&self) { if let Some(mapping) = &self.mapping { mapping.block().stop(); } }
    pub fn show(&self, ui: &mut egui::Ui, enabled: bool) {
        ui.horizontal_wrapped(|ui| {
            ui.strong("Audio visualiser");
            ui.colored_label(LEFT, egui::RichText::new("● Left").small());
            ui.colored_label(RIGHT, egui::RichText::new("● Right").small());
            let status = if self.mapping.is_none() { "Waiting for audio visualiser connection".into() }
                else if self.captured_ms == 0 { "Waiting for playback".into() }
                else if enabled { format!("Live · {:.1} kHz", self.rate / 1000.0) }
                else { format!("Live · EQ bypassed · {:.1} kHz", self.rate / 1000.0) };
            ui.label(egui::RichText::new(status).small().color(Color32::from_rgb(96, 109, 126)));
        });
        ui.columns(2, |columns| {
            plot(&mut columns[0], "Original frequencies", "Before EQ", &self.input, self.rate);
            plot(&mut columns[1], "New frequencies", "After EQ, gain & balance", &self.output, self.rate);
        });
    }
}
impl Drop for Visualiser { fn drop(&mut self) { self.stop(); } }

fn plot(ui: &mut egui::Ui, title: &str, subtitle: &str, bins: &[[f32; BINS]; 2], rate: f64) {
    let (response, painter) = ui.allocate_painter(Vec2::new(ui.available_width(), 173.0), Sense::hover());
    let outer = response.rect;
    let rect = Rect::from_min_max(outer.min + Vec2::new(36.0, 39.0), outer.max - Vec2::new(14.0, 24.0));
    painter.rect_filled(outer, 5.0, Color32::from_rgb(24, 32, 43));
    let text = Color32::from_rgb(157, 173, 191); let grid = Color32::from_rgb(48, 59, 73);
    painter.text(outer.min + Vec2::new(12.0, 9.0), Align2::LEFT_TOP, title, FontId::proportional(12.5), Color32::from_rgb(225, 232, 241));
    painter.text(Pos2::new(outer.right() - 12.0, outer.top() + 11.0), Align2::RIGHT_TOP, subtitle, FontId::proportional(10.0), text);
    let max = 20000.0f64.min(rate / 2.0);
    let x = |f: f64| rect.left() + ((f / 20.0).ln() / (max / 20.0).ln()) as f32 * rect.width();
    let y = |db: f32| rect.bottom() - ((db.clamp(FLOOR, 0.0) - FLOOR) / -FLOOR) * rect.height();
    for db in [0, -24, -48, -72, -96] {
        let level = y(db as f32);
        painter.line_segment([Pos2::new(rect.left(), level), Pos2::new(rect.right(), level)], Stroke::new(0.6, grid));
        painter.text(Pos2::new(outer.left() + 5.0, level), Align2::LEFT_CENTER, format!("{db}"), FontId::proportional(9.0), text);
    }
    painter.text(Pos2::new(outer.left() + 5.0, outer.top() + 24.0), Align2::LEFT_TOP, "dBFS", FontId::proportional(8.5), text);
    for f in [20.0, 100.0, 1000.0, 10000.0, 20000.0] {
        if f > max { continue; }
        // Avoid overlapping the last two labels on narrower windows.
        if f == 10000.0 && rect.width() < 400.0 && max >= 20000.0 { continue; }
        let px = x(f);
        painter.line_segment([Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())], Stroke::new(0.6, grid));
        let label = if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{f:.0}") };
        painter.text(Pos2::new(px, outer.bottom() - 7.0), Align2::CENTER_BOTTOM, label, FontId::proportional(9.5), text);
    }
    let clipped = painter.with_clip_rect(rect.expand(1.0));
    let count = rect.width().max(2.0) as usize;
    for ear in 0..2 {
        let points = (0..=count).map(|i| {
            let f = 20.0 * (max / 20.0).powf(i as f64 / count as f64);
            let next = 20.0 * (max / 20.0).powf((i + 1) as f64 / count as f64);
            let low = ((f * FFT_SIZE as f64 / rate).round() as usize).clamp(1, BINS - 1);
            let high = ((next * FFT_SIZE as f64 / rate).round() as usize).clamp(low, BINS - 1);
            let db = bins[ear][low..=high].iter().copied().fold(FLOOR, f32::max);
            Pos2::new(x(f), y(db))
        }).collect();
        clipped.add(egui::Shape::line(points, Stroke::new(1.5, if ear == 0 { LEFT } else { RIGHT })));
    }
    if let Some(pointer) = response.hover_pos().filter(|p| rect.contains(*p)) {
        let f = 20.0 * (max / 20.0).powf(((pointer.x - rect.left()) / rect.width()) as f64);
        let bin = ((f * FFT_SIZE as f64 / rate).round() as usize).clamp(1, BINS - 1);
        painter.line_segment([Pos2::new(pointer.x, rect.top()), Pos2::new(pointer.x, rect.bottom())], Stroke::new(0.7, text));
        response.on_hover_text(format!("{f:.0} Hz\nLeft: {:.1} dBFS\nRight: {:.1} dBFS", bins[0][bin], bins[1][bin]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fft_finds_known_tones_and_measures_gain_with_matching_calibration() {
        let tone: [f32; FFT_SIZE] = std::array::from_fn(|i| (std::f32::consts::TAU * 128.0 * i as f32 / FFT_SIZE as f32).sin() * 0.5);
        let original = frequencies(&tone); let processed = frequencies(&tone.map(|v| v * 0.5));
        let peak = original.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert_eq!(peak, 128); assert!((original[peak] + 6.0206).abs() < 0.01);
        assert!((processed[peak] - original[peak] + 6.0206).abs() < 0.01);
        assert!(frequencies(&[0.0; FFT_SIZE]).iter().all(|v| v.is_finite() && *v <= FLOOR));
    }
    #[test]
    fn visualiser_clears_when_the_output_changes_or_editor_is_hidden() {
        let mut view = Visualiser::default(); view.input[0].fill(-10.0); view.captured_ms = 1;
        view.poll(None, false); assert_eq!(view.captured_ms, 0); assert!(view.input[0].iter().all(|v| *v == FLOOR));
        view.input[0].fill(-10.0); view.poll(Some("new-output"), true);
        assert!(view.input[0].iter().all(|v| *v == FLOOR));
    }
}
