//! Two cascaded biquad filters (a 4th-order low- or high-pass), as the original's
//! `BiquadFilter` / `CrossfadeFilterAudioProcessor` use for the DJ-style crossfade.

pub const BUTTERWORTH_Q: f32 = 0.707;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterType {
    LowPass,
    HighPass,
}

#[derive(Clone, Copy, Default)]
struct Stage {
    x1: [f64; 2],
    x2: [f64; 2],
    y1: [f64; 2],
    y2: [f64; 2],
}

#[derive(Clone)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    s1: Stage,
    s2: Stage,
    pub enabled: bool,
    pub kind: FilterType,
    pub cutoff: f32,
    dirty: bool,
    sample_rate: u32,
}

impl Biquad {
    pub fn new(sample_rate: u32) -> Self {
        Biquad {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            s1: Stage::default(),
            s2: Stage::default(),
            enabled: false,
            kind: FilterType::LowPass,
            cutoff: 20000.0,
            dirty: true,
            sample_rate,
        }
    }

    pub fn set(&mut self, kind: FilterType, cutoff: f32) {
        if self.kind != kind || self.cutoff != cutoff {
            self.kind = kind;
            self.cutoff = cutoff;
            self.dirty = true;
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        if self.enabled != on {
            self.enabled = on;
            if !on {
                self.reset();
            }
        }
    }

    pub fn reset(&mut self) {
        self.s1 = Stage::default();
        self.s2 = Stage::default();
    }

    /// RBJ cookbook coefficients; the cutoff is kept between 20 Hz and Nyquist − 1.
    fn update(&mut self) {
        let sr = self.sample_rate as f32;
        let fc = self.cutoff.clamp(20.0, sr / 2.0 - 1.0) as f64;
        let w = fc * std::f64::consts::TAU / sr as f64;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (BUTTERWORTH_Q as f64 * 2.0);
        let (b0, b1, b2) = match self.kind {
            FilterType::LowPass => {
                let d = 1.0 - cos;
                (d / 2.0, d, d / 2.0)
            }
            FilterType::HighPass => {
                let d = 1.0 + cos;
                (d / 2.0, -d, d / 2.0)
            }
        };
        let a0 = 1.0 + alpha;
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = (-2.0 * cos) / a0;
        self.a2 = (1.0 - alpha) / a0;
        self.dirty = false;
    }

    #[inline]
    fn stage(&self, s: &mut Stage, ch: usize, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * s.x1[ch] + self.b2 * s.x2[ch] - self.a1 * s.y1[ch] - self.a2 * s.y2[ch];
        s.x2[ch] = s.x1[ch];
        s.x1[ch] = x;
        s.y2[ch] = s.y1[ch];
        s.y1[ch] = y;
        y
    }

    /// Filter one stereo frame in place (output clamped to −1…1).
    #[inline]
    pub fn process(&mut self, l: &mut f32, r: &mut f32) {
        if !self.enabled {
            return;
        }
        if self.dirty {
            self.update();
        }
        let mut s1 = self.s1;
        let mut s2 = self.s2;
        let yl = self.stage(&mut s2, 0, self.stage(&mut s1, 0, *l as f64));
        let yr = self.stage(&mut s2, 1, self.stage(&mut s1, 1, *r as f64));
        self.s1 = s1;
        self.s2 = s2;
        *l = yl.clamp(-1.0, 1.0) as f32;
        *r = yr.clamp(-1.0, 1.0) as f32;
    }
}

/// `1 / (1 + e^(−k(t − 0.5)))` with k = 6 (`SongPlayer.sigmoid`).
pub fn sigmoid(t: f32) -> f32 {
    1.0 / ((-(t - 0.5) * 6.0).exp() + 1.0)
}

/// Exponential interpolation between two frequencies (`SongPlayer.expInterpolate`).
pub fn exp_interpolate(start: f32, end: f32, t: f32) -> f32 {
    if start <= 0.0 || end <= 0.0 {
        return end;
    }
    (start.ln() + (end.ln() - start.ln()) * t).exp()
}
