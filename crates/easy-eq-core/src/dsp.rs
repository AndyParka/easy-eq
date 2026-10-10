use crate::{model::{finite, Band, FilterKind, Profile, MAX_BANDS}, Error};

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Coefficients { pub b0: f64, pub b1: f64, pub b2: f64, pub a1: f64, pub a2: f64 }
impl Coefficients {
    pub const IDENTITY: Self = Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };
    /// RBJ minimum-phase coefficients. Shelf frequencies refer to the midpoint.
    pub fn for_band(band: &Band, rate: f64) -> Result<Self, Error> {
        band.validate()?; finite(rate, 8000.0, 768000.0, "Sample rate")?;
        if !band.enabled { return Ok(Self::IDENTITY); }
        let omega = 2.0 * std::f64::consts::PI * band.frequency.min(rate * 0.49) / rate;
        let c = omega.cos(); let s = omega.sin();
        let a = 10f64.powf(band.gain / 40.0); let alpha = s / (2.0 * band.q);
        let beta = 2.0 * a.sqrt() * alpha;
        let (b0, b1, b2, a0, a1, a2) = match band.kind {
            FilterKind::Peak => (1.0 + alpha*a, -2.0*c, 1.0-alpha*a,
                                1.0 + alpha/a, -2.0*c, 1.0-alpha/a),
            FilterKind::LowShelf => (a*((a+1.0)-(a-1.0)*c+beta), 2.0*a*((a-1.0)-(a+1.0)*c),
                a*((a+1.0)-(a-1.0)*c-beta), (a+1.0)+(a-1.0)*c+beta,
                -2.0*((a-1.0)+(a+1.0)*c), (a+1.0)+(a-1.0)*c-beta),
            FilterKind::HighShelf => (a*((a+1.0)+(a-1.0)*c+beta), -2.0*a*((a-1.0)+(a+1.0)*c),
                a*((a+1.0)+(a-1.0)*c-beta), (a+1.0)-(a-1.0)*c+beta,
                2.0*((a-1.0)-(a+1.0)*c), (a+1.0)-(a-1.0)*c-beta),
        };
        Ok(Self { b0: b0/a0, b1: b1/a0, b2: b2/a0, a1: a1/a0, a2: a2/a0 })
    }
    pub fn magnitude_db(self, frequency: f64, rate: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * frequency / rate;
        let (s1, c1) = w.sin_cos(); let (s2, c2) = (2.0*w).sin_cos();
        let nr = self.b0 + self.b1*c1 + self.b2*c2; let ni = -self.b1*s1-self.b2*s2;
        let dr = 1.0+self.a1*c1+self.a2*c2; let di = -self.a1*s1-self.a2*s2;
        10.0 * ((nr*nr+ni*ni)/(dr*dr+di*di)).max(1e-20).log10()
    }
}

#[derive(Clone, Copy)]
struct Biquad { c: Coefficients, z1: f64, z2: f64 }
impl Biquad {
    const IDENTITY: Self = Self { c: Coefficients::IDENTITY, z1: 0.0, z2: 0.0 };
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.c.b0*x+self.z1;
        self.z1 = self.c.b1*x-self.c.a1*y+self.z2;
        self.z2 = self.c.b2*x-self.c.a2*y;
        if self.z1.abs() < 1e-30 { self.z1 = 0.0; }
        if self.z2.abs() < 1e-30 { self.z2 = 0.0; }
        y
    }
}

/// Compiled off the audio thread. Fixed-size storage avoids callback allocations.
#[derive(Clone, Copy)]
pub struct PreparedGraph {
    filters: [[Biquad; MAX_BANDS]; 2],
    counts: [usize; 2],
    matrix: [f64; 4],
    gain: f64,
    pub rate: f64,
    pub headroom_db: f64,
}
/// Fixed-size coefficient transport, with no mutable filter history.
/// Adapters compile on the control thread and validate at the audio boundary.
#[derive(Clone, Copy)]
pub struct GraphParameters {
    pub coefficients: [[Coefficients; MAX_BANDS]; 2],
    pub counts: [usize; 2],
    pub matrix: [f64; 4],
    pub gain: f64,
    pub rate: f64,
}
impl PreparedGraph {
    pub fn parameters(&self) -> GraphParameters {
        GraphParameters { coefficients: self.filters.map(|ear| ear.map(|b| b.c)),
            counts: self.counts, matrix: self.matrix, gain: self.gain, rate: self.rate }
    }
    /// Bounded, allocation-free validation of data arriving from an adapter.
    pub fn from_parameters(p: GraphParameters) -> Option<Self> {
        if !p.rate.is_finite() || !(8000.0..=768000.0).contains(&p.rate)
            || p.counts.iter().any(|&n| n > MAX_BANDS)
            || !p.gain.is_finite() || !(0.0..=1.0).contains(&p.gain)
            || p.matrix.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) { return None; }
        let mut graph = Self { filters: [[Biquad::IDENTITY; MAX_BANDS]; 2], counts: p.counts,
            matrix: p.matrix, gain: p.gain, rate: p.rate, headroom_db: 0.0 };
        for ear in 0..2 { for i in 0..p.counts[ear] {
            let c = p.coefficients[ear][i];
            if [c.b0,c.b1,c.b2,c.a1,c.a2].iter().any(|v| !v.is_finite())
                || c.a2.abs() >= 1.0 || 1.0+c.a1+c.a2 <= 0.0 || 1.0-c.a1+c.a2 <= 0.0 { return None; }
            graph.filters[ear][i].c = c;
        }}
        Some(graph)
    }
    pub fn identity(rate: f64) -> Result<Self, Error> {
        finite(rate, 8000.0, 768000.0, "Sample rate")?;
        Ok(Self { filters: [[Biquad::IDENTITY; MAX_BANDS]; 2], counts: [0,0],
            matrix: [1.0,0.0,0.0,1.0], gain: 1.0, rate, headroom_db: 0.0 })
    }
    pub fn compile(profile: &Profile, rate: f64) -> Result<Self, Error> {
        profile.validate()?;
        let mut graph = Self::identity(rate)?;
        graph.matrix = profile.matrix();
        for (ear, bands) in [&profile.ears.left, &profile.ears.right].into_iter().enumerate() {
            // The existing fixed coefficient transport can carry the shared
            // stage first in both channels without changing the installed ABI.
            for band in profile.common.iter().chain(bands).filter(|b| b.enabled) {
                graph.filters[ear][graph.counts[ear]].c = Coefficients::for_band(band, rate)?;
                graph.counts[ear] += 1;
            }
        }
        if profile.auto_headroom {
            let mix_boost = 20.0 * (graph.matrix[0]+graph.matrix[1]).max(graph.matrix[2]+graph.matrix[3]).max(1.0).log10();
            let mut peak: f64 = 0.0;
            for ear in 0..2 {
                let response = |f| mix_boost + graph.filters[ear][..graph.counts[ear]].iter().map(|b| b.c.magnitude_db(f, rate)).sum::<f64>();
                // Include DC and Nyquist: shelving gain beyond 20 kHz still affects headroom.
                peak = peak.max(response(0.0)).max(response(rate / 2.0));
                for i in 0..=2048 {
                    peak = peak.max(response(10.0 * (rate*0.499/10.0).powf(i as f64 / 2048.0)));
                }
                for band in profile.common.iter().chain(if ear == 0 { &profile.ears.left } else { &profile.ears.right }) {
                    if band.enabled { peak = peak.max(response(band.frequency.min(rate*0.49))); }
                }
            }
            if peak > 0.01 { graph.headroom_db = -((peak+1.0)*10.0).ceil()/10.0; }
        }
        graph.gain = 10f64.powf((profile.gain_db+graph.headroom_db)/20.0);
        Ok(graph)
    }
    #[inline]
    fn process(&mut self, frame: [f64; 2]) -> [f64; 2] {
        let mut output = [(self.matrix[0]*frame[0]+self.matrix[1]*frame[1])*self.gain,
                          (self.matrix[2]*frame[0]+self.matrix[3]*frame[1])*self.gain];
        for (ear, sample) in output.iter_mut().enumerate() {
            for b in &mut self.filters[ear][..self.counts[ear]] { *sample = b.process(*sample); }
        }
        output
    }
}

/// No lookahead, delayed frames, locks, file access, or renderer calls in processing.
pub struct Engine {
    current: PreparedGraph,
    previous: Option<PreparedGraph>,
    pending: Option<PreparedGraph>,
    remaining: usize,
    transition_frames: usize,
}
impl Engine {
    pub fn new(initial: PreparedGraph) -> Self {
        Self { transition_frames: (initial.rate * 0.005).round() as usize,
            current: initial, previous: None, pending: None, remaining: 0 }
    }
    /// Call only from the owning native audio host/callback boundary, never concurrently.
    /// Publish precompiled graphs via a bounded queue in each platform adapter.
    pub fn set_prepared(&mut self, graph: PreparedGraph) -> Result<(), Error> {
        if graph.rate != self.current.rate { return Err(Error("Recreate engine when device sample rate changes".into())); }
        if self.remaining > 0 { self.pending = Some(graph); }
        else { self.begin_transition(graph); }
        Ok(())
    }
    fn begin_transition(&mut self, graph: PreparedGraph) {
        self.previous = Some(self.current); self.current = graph; self.remaining = self.transition_frames;
    }
    #[inline]
    pub fn process_frame(&mut self, frame: [f64; 2]) -> [f64; 2] {
        let next = self.current.process(frame);
        if self.remaining == 0 { return next; }
        let old = self.previous.as_mut().expect("transition has previous graph").process(frame);
        let blend = 1.0 - self.remaining as f64 / self.transition_frames as f64;
        let result = [old[0]*(1.0-blend)+next[0]*blend, old[1]*(1.0-blend)+next[1]*blend];
        self.remaining -= 1;
        if self.remaining == 0 {
            self.previous = None;
            if let Some(graph) = self.pending.take() { self.begin_transition(graph); }
        }
        result
    }
    pub fn process_interleaved(&mut self, samples: &mut [f32]) -> Result<(), Error> {
        if samples.len()%2 != 0 { return Err(Error("Stereo buffer has an incomplete frame".into())); }
        for frame in samples.chunks_exact_mut(2) {
            let result = self.process_frame([frame[0] as f64, frame[1] as f64]);
            frame[0] = result[0] as f32; frame[1] = result[1] as f32;
        }
        Ok(())
    }
}

/// The platform driver must obtain this from the audio host's monotonic lease.
/// A renderer heartbeat must never renew this lease. No IPC is performed here.
#[derive(Debug, Clone, Copy, Default)]
pub struct LeaseSnapshot { pub enabled: bool, pub expires_at_nanos: u64 }
impl LeaseSnapshot {
    pub fn live(self, now_nanos: u64) -> bool { self.enabled && now_nanos < self.expires_at_nanos }
}
/// Shared fail-open policy; this alone is not a running driver or background service.
pub struct LeasedProcessor { engine: Engine, desired: PreparedGraph, identity: PreparedGraph, live: bool }
impl LeasedProcessor {
    pub fn new(desired: PreparedGraph) -> Result<Self, Error> {
        let identity = PreparedGraph::identity(desired.rate)?;
        Ok(Self { engine: Engine::new(identity), desired, identity, live: false })
    }
    pub fn set_profile(&mut self, desired: PreparedGraph) -> Result<(), Error> {
        if desired.rate != self.identity.rate { return Err(Error("Recreate processor when device rate changes".into())); }
        self.desired = desired;
        if self.live { self.engine.set_prepared(desired)?; }
        Ok(())
    }
    pub fn process_interleaved(&mut self, samples: &mut [f32], lease: LeaseSnapshot, now_nanos: u64) -> Result<(), Error> {
        self.begin_block(lease, now_nanos)?;
        self.engine.process_interleaved(samples)
    }
    /// Planar adapters use the lease once per block, then process frames without
    /// interleaving or allocating temporary buffers.
    pub fn begin_block(&mut self, lease: LeaseSnapshot, now_nanos: u64) -> Result<(), Error> {
        let live = lease.live(now_nanos);
        if live != self.live {
            self.engine.set_prepared(if live { self.desired } else { self.identity })?;
            self.live = live;
        }
        Ok(())
    }
    pub fn process_frame(&mut self, frame: [f64; 2]) -> [f64; 2] { self.engine.process_frame(frame) }
}
