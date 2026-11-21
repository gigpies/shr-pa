//! Single-output feedback guard. All state and per-sample work are bounded.
use crate::{config::EqBand, dsp::Biquad};

#[derive(Default)]
struct Window {
    samples: u32,
    energy: f64,
    difference_energy: f64,
    first: f64,
    last: f64,
    crossings: u32,
}

#[derive(Default)]
pub(crate) struct Feedback {
    pub(crate) bank: Vec<(f64, Biquad)>,
    window: Window,
    previous_sample: f64,
    previous_energy: f64,
    previous_hz: f64,
    growth_windows: u32,
    pub(crate) hz: f64,
    filter: Option<Biquad>,
}

impl Feedback {
    pub(crate) fn prepare(rate: u32, enabled: bool) -> Self {
        let mut guard = Self::default();
        if enabled {
            // One-eighth semitone spacing, built on the controller.
            for i in 0..=526 {
                let hz = 180. * 2_f64.powf(f64::from(i) / 96.);
                if hz >= 0.49 * f64::from(rate) {
                    break;
                }
                guard.bank.push((
                    hz,
                    Biquad::eq(
                        EqBand {
                            hz,
                            q: 16.,
                            db: -12.,
                            ..EqBand::default()
                        },
                        rate,
                    ),
                ));
            }
        }
        guard
    }

    pub(crate) fn reset_detection(&mut self) {
        self.window = Window::default();
        self.previous_sample = 0.;
        self.previous_energy = 0.;
        self.previous_hz = 0.;
        self.growth_windows = 0;
    }

    pub(crate) fn tick(&mut self, x: f64, rate: u32, listening: bool) -> f64 {
        if let Some(filter) = &mut self.filter {
            return filter.tick(x);
        }
        if !listening {
            return x;
        }
        let w = &mut self.window;
        w.energy += x * x;
        let delta = x - self.previous_sample;
        w.difference_energy += delta * delta;
        if x > 0. && self.previous_sample <= 0. {
            let crossing = f64::from(w.samples) - x / delta;
            if w.crossings == 0 {
                w.first = crossing;
            }
            w.last = crossing;
            w.crossings += 1;
        }
        self.previous_sample = x;
        w.samples += 1;
        if w.samples < rate / 25 {
            return x;
        }
        let w = std::mem::take(w);
        let hz = if w.crossings >= 4 {
            f64::from(rate) * f64::from(w.crossings - 1) / (w.last - w.first)
        } else {
            0.
        };
        // A sinusoid has a predictable first-difference energy. Extra harmonics
        // increase it, even if the waveform has regular zero crossings.
        let expected = 4. * (std::f64::consts::PI * hz / f64::from(rate)).sin().powi(2);
        let tonal = (w.difference_energy - expected * w.energy).abs() < 0.12 * expected * w.energy;
        let growing = w.energy > 1.32 * self.previous_energy;
        let stable = (hz - self.previous_hz).abs() < 0.012 * hz;
        if (180.0..=8000.0_f64.min(0.48 * f64::from(rate))).contains(&hz)
            && tonal
            && growing
            && stable
        {
            self.growth_windows = self.growth_windows.saturating_add(1);
        } else {
            self.growth_windows = 0;
        }
        self.previous_hz = hz;
        self.previous_energy = w.energy;
        if self.growth_windows >= 3 && w.energy > f64::from(w.samples) * 1e-6 {
            let upper = self
                .bank
                .partition_point(|(center, _)| *center < hz)
                .min(self.bank.len() - 1);
            let lower = upper.saturating_sub(1);
            let selected = if hz - self.bank[lower].0 < self.bank[upper].0 - hz {
                lower
            } else {
                upper
            };
            let (center, filter) = self.bank[selected];
            self.hz = center;
            self.filter = Some(filter);
        }
        x
    }
}
