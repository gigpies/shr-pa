//! Bounded offline log-sweep deconvolution. No capture/playback or onset realignment.
use crate::{
    calibration::Calibration,
    measurement::{CaptureMeta, Measurement, Options, SpectralBin},
};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweepSpec {
    pub start_hz: f64,
    pub end_hz: f64,
    pub sweep_samples: usize,
    pub total_samples: usize,
    pub amplitude: f64,
}
impl SweepSpec {
    pub fn excitation(&self) -> Result<Vec<f64>, &'static str> {
        if ![self.start_hz, self.end_hz, self.amplitude]
            .iter()
            .all(|v| v.is_finite())
            || self.start_hz < 20.
            || self.end_hz > 20000.
            || self.end_hz / self.start_hz < 4.
            || !(8192..=32768).contains(&self.sweep_samples)
            || ![32768, 65536].contains(&self.total_samples)
            || self.total_samples < self.sweep_samples + 4096
            || !(0.001..=0.25).contains(&self.amplitude)
        {
            return Err("sweep_limits");
        }
        let duration = self.sweep_samples as f64 / 48000.;
        let log = (self.end_hz / self.start_hz).ln();
        Ok((0..self.total_samples)
            .map(|i| {
                if i >= self.sweep_samples {
                    return 0.;
                }
                let t = i as f64 / 48000.;
                let fade = (i.min(self.sweep_samples - 1 - i) as f64 / 256.).min(1.);
                self.amplitude
                    * fade
                    * (2. * PI * self.start_hz * duration / log * ((t * log / duration).exp() - 1.))
                        .sin()
            })
            .collect())
    }
}
// Iterative radix-two DFT: fixed maximum 65536 samples; used only offline.
fn fft(x: &mut [(f64, f64)], inverse: bool) {
    let n = x.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            x.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let w = if inverse {
            2. * PI / len as f64
        } else {
            -2. * PI / len as f64
        };
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (c, s) = (w * k as f64).cos_sin();
                let b = x[start + k + len / 2];
                let v = (b.0 * c - b.1 * s, b.0 * s + b.1 * c);
                let a = x[start + k];
                x[start + k] = (a.0 + v.0, a.1 + v.1);
                x[start + k + len / 2] = (a.0 - v.0, a.1 - v.1);
            }
        }
        len *= 2;
    }
    if inverse {
        for v in x {
            v.0 /= n as f64;
            v.1 /= n as f64;
        }
    }
}
trait CosSin {
    fn cos_sin(self) -> (f64, f64);
}
impl CosSin for f64 {
    fn cos_sin(self) -> (f64, f64) {
        (self.cos(), self.sin())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweepResult {
    pub contract: String,
    pub version: u32,
    pub transfer: Measurement,
    pub calibration: Option<Calibration>,
    pub impulse: Vec<f64>,
    pub reasons: Vec<String>,
}
pub fn extract(
    meta: &CaptureMeta,
    spec: &SweepSpec,
    reference: &[f64],
    response: &[f64],
    calibration: Option<&Calibration>,
) -> Result<SweepResult, &'static str> {
    let expected = spec.excitation()?;
    let n = expected.len();
    crate::measurement::validate_meta(meta, n)?;
    if reference.len() != n
        || response.len() != n
        || reference
            .iter()
            .zip(&expected)
            .any(|(a, b)| !a.is_finite() || (a - b).abs() > 1e-10)
        || response.iter().any(|v| !v.is_finite() || v.abs() >= 0.999)
    {
        return Err("sweep_shape_clipping_or_excitation_timing");
    }
    if let Some(c) = calibration {
        c.validate()?;
    }
    let power = response.iter().map(|v| v * v).sum::<f64>() / n as f64;
    let tail = response[n - 2048..].iter().map(|v| v * v).sum::<f64>() / 2048.;
    if power < 1e-10 || tail > power * 1e-4 {
        return Err("sweep_noise_or_truncated_tail");
    }
    let mut x: Vec<_> = reference.iter().map(|&v| (v, 0.)).collect();
    let mut y: Vec<_> = response.iter().map(|&v| (v, 0.)).collect();
    fft(&mut x, false);
    fft(&mut y, false);
    let peak = x.iter().map(|v| v.0 * v.0 + v.1 * v.1).fold(0., f64::max);
    let mut h = vec![(0., 0.); n];
    for i in 0..n {
        let p = x[i].0 * x[i].0 + x[i].1 * x[i].1;
        if p > peak * 1e-6 {
            h[i] = (
                (y[i].0 * x[i].0 + y[i].1 * x[i].1) / p,
                (y[i].1 * x[i].0 - y[i].0 * x[i].1) / p,
            );
        }
    }
    let mut impulse = h.clone();
    fft(&mut impulse, true);
    let arrival = impulse
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.0.abs().total_cmp(&b.1.0.abs()))
        .unwrap()
        .0;
    if arrival >= 2048 {
        return Err("sweep_timing_outside_common_origin");
    }
    let mut spectrum = Vec::new();
    let low = spec.start_hz * 1.25;
    let high = spec.end_hz / 1.25;
    for k in 0..64 {
        let hz = low * (high / low).powf(k as f64 / 63.);
        let bin = ((hz * n as f64 / 48000.).round() as usize).clamp(
            (low * n as f64 / 48000.).ceil() as usize,
            (high * n as f64 / 48000.).floor() as usize,
        );
        let hz = bin as f64 * 48000. / n as f64;
        let p = x[bin].0 * x[bin].0 + x[bin].1 * x[bin].1;
        if p < peak * 1e-4 {
            continue;
        }
        let correction = match calibration {
            Some(c) => c.correction_db(hz)?,
            None => 0.,
        };
        spectrum.push(SpectralBin {
            frequency_hz: hz,
            magnitude: h[bin].0.hypot(h[bin].1) * 10_f64.powf(correction / 20.),
            phase_radians: h[bin].1.atan2(h[bin].0),
            coherence: 0.,
        });
    }
    // A single sweep cannot estimate magnitude-squared coherence. Explicitly refuse
    // automatic fitting until independent repeat evidence supplies reliability.
    let reasons = vec!["single_sweep_coherence_unavailable".into()];
    Ok(SweepResult {
        contract: "C-PA-SWEEP-RESULT".into(),
        version: 1,
        transfer: Measurement {
            contract: "C-PA-MEASUREMENT-RESULT".into(),
            version: 1,
            algorithm: "offline-sweep-v1".into(),
            capture: meta.clone(),
            options: Options {
                max_arrival_samples: 2048,
                band_hz: [low, high],
            },
            samples: n,
            arrival_samples: arrival as i32,
            signed_correlation: 0.,
            competing_peak_ratio: 0.,
            segments: 1,
            spectrum,
            proposal_eligible: false,
            reasons: reasons.clone(),
        },
        calibration: calibration.cloned(),
        impulse: impulse[..4096].iter().map(|v| v.0).collect(),
        reasons,
    })
}

/// Four separately acquired responses with identical common-origin timing.
/// Ensemble coherence is computed without independently shifting any response.
pub fn extract_repeated(
    meta: &CaptureMeta,
    spec: &SweepSpec,
    reference: &[f64],
    responses: &[Vec<f64>],
    calibration: Option<&Calibration>,
) -> Result<SweepResult, &'static str> {
    if responses.len() != 4 {
        return Err("four_sweep_repeats_required");
    }
    let mut results = responses
        .iter()
        .map(|r| extract(meta, spec, reference, r, calibration))
        .collect::<Result<Vec<_>, _>>()?;
    let mut result = results.remove(0);
    for other in &results {
        if other.transfer.arrival_samples != result.transfer.arrival_samples {
            return Err("sweep_repeat_timing_conflict");
        }
    }
    for (i, bin) in result.transfer.spectrum.iter_mut().enumerate() {
        let mut re = bin.magnitude * bin.phase_radians.cos();
        let mut im = bin.magnitude * bin.phase_radians.sin();
        let mut power = bin.magnitude.powi(2);
        for other in &results {
            let b = other.transfer.spectrum.get(i).ok_or("sweep_repeat_band")?;
            if b.frequency_hz != bin.frequency_hz {
                return Err("sweep_repeat_band");
            }
            re += b.magnitude * b.phase_radians.cos();
            im += b.magnitude * b.phase_radians.sin();
            power += b.magnitude.powi(2);
        }
        bin.coherence = ((re * re + im * im) / (4. * power)).clamp(0., 1.);
        bin.magnitude = re.hypot(im) / 4.;
        bin.phase_radians = im.atan2(re);
    }
    result.transfer.algorithm = "offline-sweep-ensemble-v1".into();
    result.transfer.segments = 4;
    result.reasons = if result.transfer.spectrum.len() < 12
        || result.transfer.spectrum.iter().any(|b| b.coherence < 0.98)
    {
        vec!["sweep_repeat_low_coherence".into()]
    } else {
        Vec::new()
    };
    result.transfer.reasons = result.reasons.clone();
    // Delay/polarity remains a separate overlap analysis; no correlation fabricated.
    result.transfer.proposal_eligible = false;
    Ok(result)
}
