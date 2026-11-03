//! Small allocation-free effects. Buffers are sized once in `new`.

use biquad::{Biquad, Coefficients, DirectForm2Transposed, ToHertz, Type, Q_BUTTERWORTH_F32};

pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn coefficient(ms: f32, rate: f32) -> f32 {
    (-1.0 / (ms * 0.001 * rate)).exp()
}

pub struct Filter {
    inner: DirectForm2Transposed<f32>,
    pub active: bool,
}

impl Filter {
    pub fn new() -> Self {
        let pass = Coefficients { a1: 0.0, a2: 0.0, b0: 1.0, b1: 0.0, b2: 0.0 };
        Self { inner: DirectForm2Transposed::<f32>::new(pass), active: false }
    }

    /// Retunes in place, keeping the filter state so sliders do not click.
    pub fn set(&mut self, kind: Type<f32>, rate: f32, hz: f32, q: f32, active: bool) {
        let hz = hz.clamp(10.0, rate * 0.45);
        if let Ok(coefficients) = Coefficients::<f32>::from_params(kind, rate.hz(), hz.hz(), q) {
            self.inner.update_coefficients(coefficients);
        }
        if active && !self.active {
            self.inner.reset_state();
        }
        self.active = active;
    }

    pub fn highpass(&mut self, rate: f32, hz: f32) {
        self.set(Type::HighPass, rate, hz, Q_BUTTERWORTH_F32, true);
    }

    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        if self.active { self.inner.run(x) } else { x }
    }
}

/// Hysteresis gate with hold, so word endings are not chopped.
pub struct Gate {
    threshold: f32,
    envelope: f32,
    gain: f32,
    hold: usize,
    held: usize,
    env_release: f32,
    open_coef: f32,
    close_coef: f32,
}

impl Gate {
    pub fn new(rate: f32) -> Self {
        Self {
            threshold: 0.0,
            envelope: 0.0,
            gain: 0.0,
            hold: (rate * 0.12) as usize,
            held: 0,
            env_release: coefficient(40.0, rate),
            open_coef: coefficient(2.0, rate),
            close_coef: coefficient(80.0, rate),
        }
    }

    pub fn set(&mut self, threshold_db: f32) {
        self.threshold = if threshold_db <= -99.0 { 0.0 } else { db_to_gain(threshold_db) };
    }

    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let level = x.abs();
        self.envelope = if level > self.envelope {
            level
        } else {
            level + self.env_release * (self.envelope - level)
        };
        let open = if self.envelope >= self.threshold {
            self.held = self.hold;
            true
        } else if self.envelope >= self.threshold * 0.6 && self.held > 0 {
            self.held -= 1;
            true
        } else {
            self.held = self.held.saturating_sub(1);
            self.held > 0
        };
        let (target, coef) = if open { (1.0, self.open_coef) } else { (0.0, self.close_coef) };
        self.gain = target + coef * (self.gain - target);
        x * self.gain
    }
}

/// Feed-forward RMS compressor with makeup gain.
pub struct Compressor {
    power: f32,
    gain: f32,
    power_coef: f32,
    attack: f32,
    release: f32,
}

impl Compressor {
    const THRESHOLD_DB: f32 = -20.0;
    const RATIO: f32 = 3.0;
    const MAKEUP_DB: f32 = 5.0;

    pub fn new(rate: f32) -> Self {
        Self {
            power: 0.0,
            gain: 1.0,
            power_coef: coefficient(10.0, rate),
            attack: coefficient(5.0, rate),
            release: coefficient(120.0, rate),
        }
    }

    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        self.power = x * x + self.power_coef * (self.power - x * x);
        let level_db = 10.0 * (self.power + 1e-12).log10();
        let over = (level_db - Self::THRESHOLD_DB).max(0.0);
        let target = db_to_gain(-over * (1.0 - 1.0 / Self::RATIO) + Self::MAKEUP_DB);
        let coef = if target < self.gain { self.attack } else { self.release };
        self.gain = target + coef * (self.gain - target);
        x * self.gain
    }
}

/// Peak limiter: instant attack, smooth release, then a hard ceiling.
pub struct Limiter {
    gain: f32,
    release: f32,
}

impl Limiter {
    const CEILING: f32 = 0.89; // -1 dBFS

    pub fn new(rate: f32) -> Self {
        Self { gain: 1.0, release: coefficient(80.0, rate) }
    }

    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let needed = if x.abs() * self.gain > Self::CEILING { Self::CEILING / x.abs() } else { 1.0 };
        self.gain = if needed < self.gain { needed } else { 1.0 + self.release * (self.gain - 1.0) };
        (x * self.gain).clamp(-Self::CEILING, Self::CEILING)
    }
}

/// Mono chorus: one LFO-modulated delay tap.
pub struct Chorus {
    buffer: Vec<f32>,
    position: usize,
    phase: f32,
    step: f32,
    center: f32,
    depth: f32,
    pub mix: f32,
}

impl Chorus {
    pub fn new(rate: f32) -> Self {
        Self {
            buffer: vec![0.0; (rate * 0.05) as usize],
            position: 0,
            phase: 0.0,
            step: 2.0 * std::f32::consts::PI * 1.3 / rate,
            center: rate * 0.009,
            depth: rate * 0.003,
            mix: 0.0,
        }
    }

    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let size = self.buffer.len();
        self.buffer[self.position] = x;
        self.phase = (self.phase + self.step) % (2.0 * std::f32::consts::PI);
        let delay = self.center + self.depth * self.phase.sin();
        let read = self.position as f32 + size as f32 - delay;
        let low = read.floor() as usize;
        let fraction = read - low as f32;
        let wet = self.buffer[low % size] * (1.0 - fraction) + self.buffer[(low + 1) % size] * fraction;
        self.position = (self.position + 1) % size;
        x * (1.0 - self.mix * 0.5) + wet * self.mix
    }
}

/// Freeverb (Jezar, public domain), mono, tunings scaled from 44.1 kHz.
pub struct Reverb {
    combs: Vec<(Vec<f32>, usize, f32)>,
    allpasses: Vec<(Vec<f32>, usize)>,
    pub mix: f32,
}

impl Reverb {
    const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
    const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
    const FEEDBACK: f32 = 0.35 * 0.28 + 0.7; // Freeverb room size 0.35
    const DAMP: f32 = 0.7 * 0.4; // Freeverb damping 0.7

    pub fn new(rate: f32) -> Self {
        let scale = rate / 44_100.0;
        let len = |n: usize| ((n as f32 * scale) as usize).max(1);
        Self {
            combs: Self::COMBS.iter().map(|&n| (vec![0.0; len(n)], 0, 0.0)).collect(),
            allpasses: Self::ALLPASSES.iter().map(|&n| (vec![0.0; len(n)], 0)).collect(),
            mix: 0.0,
        }
    }

    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let input = x * 0.015;
        let mut wet = 0.0;
        for (buffer, index, store) in &mut self.combs {
            let out = buffer[*index];
            *store = out * (1.0 - Self::DAMP) + *store * Self::DAMP;
            buffer[*index] = input + *store * Self::FEEDBACK;
            *index = (*index + 1) % buffer.len();
            wet += out;
        }
        for (buffer, index) in &mut self.allpasses {
            let delayed = buffer[*index];
            buffer[*index] = wet + delayed * 0.5;
            wet = delayed - wet;
            *index = (*index + 1) % buffer.len();
        }
        x + wet * self.mix * 3.0
    }
}
