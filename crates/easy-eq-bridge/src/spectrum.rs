//! Optional, bounded audio telemetry. This is separate from the coefficient
//! protocol so older installed audio components keep their existing behavior.
use std::sync::atomic::{AtomicU64, Ordering};

pub const FFT_SIZE: usize = 4096;
const MAGIC: u64 = 0x5032535045435401;

pub struct SpectrumFrame {
    pub input: [[f32; FFT_SIZE]; 2],
    pub output: [[f32; FFT_SIZE]; 2],
    pub rate: f64,
    pub captured_ms: u64,
    pub pid: u64,
}

#[repr(C, align(8))]
pub struct SpectrumBlock {
    magic: AtomicU64,
    sequence: AtomicU64,
    pub requested_until_ms: AtomicU64,
    rate: AtomicU64,
    captured_ms: AtomicU64,
    pid: AtomicU64,
    input: [AtomicU64; FFT_SIZE],
    output: [AtomicU64; FFT_SIZE],
}

impl Default for SpectrumBlock {
    fn default() -> Self {
        Self {
            magic: AtomicU64::new(MAGIC), sequence: AtomicU64::new(0),
            requested_until_ms: AtomicU64::new(0), rate: AtomicU64::new(0),
            captured_ms: AtomicU64::new(0), pid: AtomicU64::new(0),
            input: std::array::from_fn(|_| AtomicU64::new(0)),
            output: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

fn pack(frame: [f32; 2]) -> u64 { frame[0].to_bits() as u64 | ((frame[1].to_bits() as u64) << 32) }
fn unpack(word: u64) -> [f32; 2] { [f32::from_bits(word as u32), f32::from_bits((word >> 32) as u32)] }

impl SpectrumBlock {
    pub fn valid(&self) -> bool { self.magic.load(Ordering::Acquire) == MAGIC }
    pub fn request(&self, now: u64) { self.requested_until_ms.store(now.saturating_add(500), Ordering::Release); }
    pub fn stop(&self) { self.requested_until_ms.store(0, Ordering::Release); }
    pub fn requested(&self, now: u64) -> bool {
        let expiry = self.requested_until_ms.load(Ordering::Acquire);
        now < expiry && expiry.saturating_sub(now) <= 500
    }
    /// One callback writer per endpoint; never waits for the reader. Only the
    /// latest window is retained, so a slow editor drops visualization frames.
    fn publish(&self, frame: &SpectrumFrame) {
        let odd = (self.sequence.load(Ordering::SeqCst) | 1).wrapping_add(2);
        self.sequence.store(odd, Ordering::SeqCst);
        self.rate.store(frame.rate.to_bits(), Ordering::SeqCst);
        self.captured_ms.store(frame.captured_ms, Ordering::SeqCst);
        self.pid.store(frame.pid, Ordering::SeqCst);
        for i in 0..FFT_SIZE {
            self.input[i].store(pack([frame.input[0][i], frame.input[1][i]]), Ordering::SeqCst);
            self.output[i].store(pack([frame.output[0][i], frame.output[1][i]]), Ordering::SeqCst);
        }
        self.sequence.store(odd.wrapping_add(1), Ordering::SeqCst);
    }
    /// At most two attempts. Atomic payloads make overlapping accesses defined.
    pub fn read(&self, previous: u64) -> Option<(u64, SpectrumFrame)> {
        if !self.valid() { return None; }
        for _ in 0..2 {
            let sequence = self.sequence.load(Ordering::SeqCst);
            if sequence == 0 || sequence == previous || sequence & 1 != 0 { return None; }
            let mut frame = SpectrumFrame {
                input: [[0.0; FFT_SIZE]; 2], output: [[0.0; FFT_SIZE]; 2],
                rate: f64::from_bits(self.rate.load(Ordering::SeqCst)),
                captured_ms: self.captured_ms.load(Ordering::SeqCst), pid: self.pid.load(Ordering::SeqCst),
            };
            for i in 0..FFT_SIZE {
                let input = unpack(self.input[i].load(Ordering::SeqCst));
                let output = unpack(self.output[i].load(Ordering::SeqCst));
                for ear in 0..2 { frame.input[ear][i] = input[ear]; frame.output[ear][i] = output[ear]; }
            }
            if sequence != self.sequence.load(Ordering::SeqCst) { continue; }
            if !frame.rate.is_finite() || !(8000.0..=768000.0).contains(&frame.rate)
                || frame.input.iter().chain(&frame.output).flatten().any(|v| !v.is_finite()) { return None; }
            return Some((sequence, frame));
        }
        None
    }
}

/// Preallocated contiguous windows. FFTs and drawing happen in the editor,
/// outside the audio callback. Telemetry never buffers or delays playback.
pub struct SpectrumCapture { frame: SpectrumFrame, cursor: usize, skip: usize }
impl Default for SpectrumCapture {
    fn default() -> Self {
        Self { frame: SpectrumFrame { input: [[0.0; FFT_SIZE]; 2], output: [[0.0; FFT_SIZE]; 2],
            rate: 0.0, captured_ms: 0, pid: 0 }, cursor: 0, skip: 0 }
    }
}
impl SpectrumCapture {
    pub fn reset(&mut self) { self.cursor = 0; self.skip = 0; }
    pub fn push(&mut self, block: &SpectrumBlock, input: [f32; 2], output: [f32; 2], rate: f64, now: u64, pid: u64) {
        if self.skip > 0 { self.skip -= 1; return; }
        if self.frame.rate != rate { self.reset(); self.frame.rate = rate; }
        for ear in 0..2 {
            self.frame.input[ear][self.cursor] = if input[ear].is_finite() { input[ear] } else { 0.0 };
            self.frame.output[ear][self.cursor] = if output[ear].is_finite() { output[ear] } else { 0.0 };
        }
        self.cursor += 1;
        if self.cursor == FFT_SIZE {
            self.frame.captured_ms = now; self.frame.pid = pid;
            block.publish(&self.frame);
            self.cursor = 0;
            self.skip = (rate as usize / 30).saturating_sub(FFT_SIZE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_keeps_original_processed_and_stereo_samples_separate() {
        let block = SpectrumBlock::default(); let mut capture = SpectrumCapture::default();
        for i in 0..FFT_SIZE - 1 { capture.push(&block, [i as f32, -0.25], [0.5, -1.0], 48000.0, 42, 7); }
        assert!(block.read(0).is_none());
        capture.push(&block, [0.25, -0.5], [0.125, -0.75], 48000.0, 42, 7);
        let (sequence, frame) = block.read(0).unwrap();
        assert_eq!(frame.input[0][FFT_SIZE - 1], 0.25); assert_eq!(frame.input[1][FFT_SIZE - 1], -0.5);
        assert_eq!(frame.output[0][FFT_SIZE - 1], 0.125); assert_eq!(frame.output[1][FFT_SIZE - 1], -0.75);
        assert_eq!((frame.rate, frame.captured_ms, frame.pid), (48000.0, 42, 7));
        assert!(block.read(sequence).is_none());
        block.sequence.store(sequence + 1, Ordering::SeqCst); assert!(block.read(0).is_none());
    }
    #[test]
    fn capture_lease_expires_and_rate_changes_discard_partial_windows() {
        let block = SpectrumBlock::default(); block.request(100);
        assert!(block.requested(100)); assert!(!block.requested(600)); block.stop(); assert!(!block.requested(101));
        let mut capture = SpectrumCapture::default();
        capture.push(&block, [1.0; 2], [1.0; 2], 48000.0, 100, 1);
        for _ in 0..FFT_SIZE { capture.push(&block, [0.0; 2], [0.0; 2], 44100.0, 101, 1); }
        let (_, frame) = block.read(0).unwrap(); assert_eq!(frame.rate, 44100.0);
        assert!(frame.input.iter().flatten().all(|v| *v == 0.0));
    }
}
