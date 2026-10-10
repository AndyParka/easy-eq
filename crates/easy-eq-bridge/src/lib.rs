//! Coefficients are prepared by the desktop host, never inside an audio callback.
//! Atomic payload words make overlapping reads/writes well-defined. A bounded
//! sequence check avoids waiting if the control process is interrupted mid-write.
use easy_eq_core::{Coefficients, GraphParameters, PreparedGraph, MAX_BANDS};
use std::sync::atomic::{AtomicU64, Ordering};
pub mod spectrum;

const MAGIC: u64 = 0x5032425249444701;
const WORDS: usize = 9 + 2 * MAX_BANDS * 5;

#[repr(C, align(8))]
pub struct SharedBlock {
    magic: AtomicU64,
    sequence: AtomicU64,
    pub expires_ms: AtomicU64,
    pub enabled: AtomicU64,
    pub device_rate: AtomicU64,
    pub callback_ms: AtomicU64,
    pub callback_pid: AtomicU64,
    pub applied_sequence: AtomicU64,
    pub processing_enabled: AtomicU64,
    payload: [AtomicU64; WORDS],
}
impl Default for SharedBlock {
    fn default() -> Self {
        Self { magic: AtomicU64::new(MAGIC), sequence: AtomicU64::new(0), expires_ms: AtomicU64::new(0),
            enabled: AtomicU64::new(0), device_rate: AtomicU64::new(0), callback_ms: AtomicU64::new(0),
            callback_pid: AtomicU64::new(0), applied_sequence: AtomicU64::new(0), processing_enabled: AtomicU64::new(0),
            payload: std::array::from_fn(|_| AtomicU64::new(0)) }
    }
}
impl SharedBlock {
    pub fn valid(&self) -> bool { self.magic.load(Ordering::Acquire) == MAGIC }
    pub fn sequence(&self) -> u64 { self.sequence.load(Ordering::SeqCst) }
    /// Exactly one writer owns each endpoint's control mapping.
    pub fn publish(&self, graph: PreparedGraph) -> u64 {
        let p = graph.parameters();
        let mut words = [0u64; WORDS];
        words[0] = p.rate.to_bits(); words[1] = p.gain.to_bits();
        for i in 0..4 { words[2+i] = p.matrix[i].to_bits(); }
        words[6] = p.counts[0] as u64; words[7] = p.counts[1] as u64;
        // Word 8 is reserved for the next protocol version.
        let mut i = 9;
        for ear in p.coefficients { for c in ear {
            for value in [c.b0,c.b1,c.b2,c.a1,c.a2] { words[i] = value.to_bits(); i += 1; }
        }}
        let previous = self.sequence.load(Ordering::SeqCst);
        let odd = (previous | 1).wrapping_add(2);
        self.sequence.store(odd, Ordering::SeqCst);
        for (target, value) in self.payload.iter().zip(words) { target.store(value, Ordering::SeqCst); }
        let sequence = odd.wrapping_add(1); self.sequence.store(sequence, Ordering::SeqCst); sequence
    }
    /// Two attempts at most; an incomplete update leaves the last valid graph.
    pub fn read_graph(&self, previous: u64) -> Option<(u64, PreparedGraph)> {
        if !self.valid() { return None; }
        for _ in 0..2 {
            let first = self.sequence.load(Ordering::SeqCst);
            if first == 0 || first == previous || first & 1 != 0 { return None; }
            let words = self.payload.each_ref().map(|v| v.load(Ordering::SeqCst));
            if first != self.sequence.load(Ordering::SeqCst) { continue; }
            if words[6] > MAX_BANDS as u64 || words[7] > MAX_BANDS as u64 { return None; }
            let mut p = GraphParameters { coefficients: [[Coefficients::IDENTITY; MAX_BANDS]; 2],
                counts: [words[6] as usize, words[7] as usize], matrix: [0.0; 4],
                gain: f64::from_bits(words[1]), rate: f64::from_bits(words[0]) };
            for i in 0..4 { p.matrix[i] = f64::from_bits(words[2+i]); }
            let mut i = 9;
            for ear in &mut p.coefficients { for c in ear {
                *c = Coefficients { b0: f64::from_bits(words[i]), b1: f64::from_bits(words[i+1]),
                    b2: f64::from_bits(words[i+2]), a1: f64::from_bits(words[i+3]), a2: f64::from_bits(words[i+4]) }; i += 5;
            }}
            return PreparedGraph::from_parameters(p).map(|g| (first, g));
        }
        None
    }
    pub fn renew(&self, enabled: bool, now_ms: u64) {
        self.enabled.store(enabled as u64, Ordering::Release);
        self.expires_ms.store(now_ms.saturating_add(1500), Ordering::Release);
    }
    pub fn bypass(&self) { self.enabled.store(0, Ordering::Release); self.expires_ms.store(0, Ordering::Release); }
}

#[cfg(windows)]
pub mod windows;

#[cfg(test)]
mod tests {
    use super::*;
    use easy_eq_core::{Band, FilterKind, Profile, Engine};
    #[test]
    fn fixed_transport_preserves_per_ear_processing() {
        let mut p = Profile::flat("test", "Test"); p.gain_db = -6.0; p.pan = 0.25;
        p.ears.left.push(Band { id:"one".into(), kind:FilterKind::Peak, frequency:1500.0,gain:-8.0,q:1.1,enabled:true });
        let graph = PreparedGraph::compile(&p, 48000.0).unwrap();
        let block = SharedBlock::default(); let sequence = block.publish(graph);
        let (actual, copy) = block.read_graph(0).unwrap(); assert_eq!(sequence,actual);
        let mut a = Engine::new(graph); let mut b = Engine::new(copy);
        for i in 0..500 { let frame = [if i == 0 {0.5} else {0.0}, 0.125]; assert_eq!(a.process_frame(frame), b.process_frame(frame)); }
        assert!(block.read_graph(sequence).is_none());
    }
    #[test]
    fn interrupted_or_invalid_publication_does_not_wait_or_apply() {
        let block = SharedBlock::default(); block.publish(PreparedGraph::identity(48000.0).unwrap());
        block.sequence.store(7, Ordering::SeqCst); assert!(block.read_graph(0).is_none());
        block.sequence.store(8, Ordering::SeqCst); block.payload[6].store(u64::MAX, Ordering::SeqCst);
        assert!(block.read_graph(0).is_none());
        block.payload[6].store(0, Ordering::SeqCst); block.payload[1].store(f64::NAN.to_bits(), Ordering::SeqCst);
        assert!(block.read_graph(0).is_none());
    }
}
