//! Uncalibrated, independent-clock bench observations. Never produces GP20 metadata
//! or an EQ proposal. The caller supplies bounded search windows, not aligned WAVs.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
const RATE: f64 = 48000.;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub source_frame: u64,
    pub dry_search: [usize; 2],
    pub wet_search: [usize; 2],
    pub source_onset_difference: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub capture: PathBuf,
    pub channel: usize,
    pub reference: PathBuf,
    pub reference_channel: usize,
    pub template: [usize; 2],
    pub noise: [usize; 2],
    pub events: Vec<Event>,
    pub intentional_delay_frames: u64,
    pub scheduled_delay_frames: u64,
    pub frequency_bias_bound_ms: f64,
}
#[derive(Serialize, Debug)]
pub struct Arrival {
    pub frame: usize,
    pub correlation: f64,
    pub snr_db: f64,
    pub peak_dbfs: f64,
}
pub(crate) fn wav(path: &std::path::Path, channel: usize) -> Result<Vec<f64>, String> {
    let mut r = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let s = r.spec();
    if s.sample_rate != 48000
        || channel >= usize::from(s.channels)
        || s.channels > 20
        || r.duration() == 0
        || r.duration() > 48000 * 60
    {
        return Err("48k bounded WAV/channel required".into());
    }
    let samples: Vec<f64> = match (s.sample_format, s.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => r
            .samples::<i16>()
            .map(|s| s.map(|v| f64::from(v) / 32768.))
            .collect::<Result<_, _>>(),
        (hound::SampleFormat::Float, 32) => r
            .samples::<f32>()
            .map(|s| s.map(f64::from))
            .collect::<Result<_, _>>(),
        _ => return Err("PCM16 or float32 WAV required".into()),
    }
    .map_err(|e| e.to_string())?;
    if samples.iter().any(|s| !s.is_finite()) {
        return Err("nonfinite recording".into());
    }
    Ok(samples
        .into_iter()
        .skip(channel)
        .step_by(usize::from(s.channels))
        .collect())
}
pub(crate) fn slice(x: &[f64], window: [usize; 2]) -> Result<&[f64], String> {
    if window[0] >= window[1] {
        return Err("empty/reversed window".into());
    }
    x.get(window[0]..window[1])
        .ok_or_else(|| "window outside capture".into())
}
fn energy(x: &[f64]) -> f64 {
    x.iter().map(|s| s * s).sum::<f64>() / x.len() as f64
}
// 1-ms RMS envelope removes carrier phase as an arrival cue. Identical smoothing
// is applied to reference and observation; broad/reflected envelopes can refuse.
fn envelope(x: &[f64]) -> Vec<f64> {
    let mut sum = 0.;
    x.iter()
        .enumerate()
        .map(|(i, v)| {
            sum += v * v;
            if i >= 48 {
                sum -= x[i - 48] * x[i - 48];
            }
            (sum.max(0.) / 48.).sqrt()
        })
        .collect()
}
pub fn arrival(
    recording: &[f64],
    template: &[f64],
    search: [usize; 2],
    noise_rms: f64,
) -> Result<Arrival, String> {
    if !(480..=4800).contains(&template.len())
        || search[1].saturating_sub(search[0]) > 9600
        || !noise_rms.is_finite()
        || noise_rms <= 0.
    {
        return Err("arrival bounds/noise floor".into());
    }
    let data = slice(recording, search)?;
    if data.len() < template.len() + 96
        || data
            .iter()
            .any(|s| !s.is_finite() || s.abs() >= 10f64.powf(-3. / 20.))
    {
        return Err("short/clipped/nonfinite arrival window".into());
    }
    let t = envelope(template);
    let y = envelope(data);
    let te = t.iter().map(|v| v * v).sum::<f64>();
    if te <= 1e-20 {
        return Err("silent template".into());
    }
    let mut scores = Vec::with_capacity(data.len() - t.len() + 1);
    for offset in 0..=y.len() - t.len() {
        let w = &y[offset..offset + t.len()];
        let e = w.iter().map(|v| v * v).sum::<f64>();
        let cross = w.iter().zip(&t).map(|(a, b)| a * b).sum::<f64>();
        scores.push(if e > 1e-20 {
            cross / (te * e).sqrt()
        } else {
            0.
        });
    }
    let (offset, &score) = scores
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .ok_or("no arrival")?;
    if offset < 48 || offset + 48 >= scores.len() || score < 0.9 {
        return Err("weak or window-edge arrival".into());
    }
    // Distinct competing envelope peaks, not adjacent points of one smooth peak.
    if scores
        .iter()
        .enumerate()
        .any(|(i, &v)| i.abs_diff(offset) > 480 && v > score - 0.03)
    {
        return Err("ambiguous arrival/reflection".into());
    }
    let hit = &data[offset..offset + t.len()];
    let snr = 10. * (energy(hit) / (noise_rms * noise_rms)).log10();
    if snr < 15. {
        return Err("arrival below 15 dB background margin".into());
    }
    Ok(Arrival {
        frame: search[0] + offset,
        correlation: score,
        snr_db: snr,
        peak_dbfs: 20. * hit.iter().fold(0f64, |a, b| a.max(b.abs())).log10(),
    })
}
pub fn execute(r: Request) -> Result<serde_json::Value, String> {
    if !(3..=8).contains(&r.events.len())
        || !r.frequency_bias_bound_ms.is_finite()
        || !(0.0..=20.).contains(&r.frequency_bias_bound_ms)
    {
        return Err("three to eight repeats and explicit frequency-bias bound required".into());
    }
    let recording = wav(&r.capture, r.channel)?;
    let reference = wav(&r.reference, r.reference_channel)?;
    let template = slice(&reference, r.template)?;
    let noise = energy(slice(&recording, r.noise)?).sqrt();
    let mut dry = Vec::new();
    let mut wet = Vec::new();
    for e in &r.events {
        dry.push(arrival(&recording, template, e.dry_search, noise)?);
        wet.push(arrival(&recording, template, e.wet_search, noise)?);
    }
    for i in 1..r.events.len() {
        if r.events[i].source_frame <= r.events[i - 1].source_frame
            || dry[i].frame <= dry[i - 1].frame
        {
            return Err("repeat source/arrival order".into());
        }
    }
    let n = dry.len() as f64;
    let xmean = r.events.iter().map(|e| e.source_frame as f64).sum::<f64>() / n;
    let ymean = dry.iter().map(|a| a.frame as f64).sum::<f64>() / n;
    let xx = r
        .events
        .iter()
        .map(|e| (e.source_frame as f64 - xmean).powi(2))
        .sum::<f64>();
    let ratio = r
        .events
        .iter()
        .zip(&dry)
        .map(|(e, a)| (e.source_frame as f64 - xmean) * (a.frame as f64 - ymean))
        .sum::<f64>()
        / xx;
    if !ratio.is_finite() || (ratio - 1.).abs() > 0.002 {
        return Err("clock slope beyond 2000ppm or ambiguous repeats".into());
    }
    let residual = dry
        .iter()
        .zip(&r.events)
        .map(|(a, e)| (a.frame as f64 - ymean - ratio * (e.source_frame as f64 - xmean)).abs())
        .fold(0f64, f64::max);
    let differences: Vec<f64> = dry
        .iter()
        .zip(&wet)
        .zip(&r.events)
        .map(|((a, b), e)| {
            ((b.frame as f64 - a.frame as f64) / ratio - e.source_onset_difference as f64) / RATE
                * 1000.
        })
        .collect();
    let mean = differences.iter().sum::<f64>() / n;
    let spread = differences
        .iter()
        .map(|d| (d - mean).abs())
        .fold(0f64, f64::max);
    Ok(
        serde_json::json!({"contract":"SHR-PA-ACOUSTIC-OBSERVATION","version":1,"dry":dry,"wet":wet,"relative_wet_ms":differences,"mean_relative_wet_ms":mean,"repeat_max_deviation_ms":spread,"capture_per_source_frame":ratio,"estimated_drift_ppm":(ratio-1.)*1e6,"dry_affine_residual_frames":residual,"intentional_effect_ms":r.intentional_delay_frames as f64/RATE*1000.,"scheduled_transport_ms":r.scheduled_delay_frames as f64/RATE*1000.,"unexplained_relative_ms":mean-(r.intentional_delay_frames+r.scheduled_delay_frames) as f64/RATE*1000.,"conservative_observation_bound_ms":spread+2.*residual/RATE*1000.+2.+r.frequency_bias_bound_ms,"absolute_latency_qualified":false,"calibrated_spl":false,"microphone_calibration":null,"eq_proposal":null,"limits":"Envelope arrival, 1ms smoothing; empirical drift estimate is not physical clock lock. Supplied frequency bias must be supported by same-probe baseline. No absolute source-to-mic latency."}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn burst() -> Vec<f64> {
        (0..2400)
            .map(|n| {
                0.01 * (0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / 2399.).cos())
                    * (std::f64::consts::TAU * n as f64 / 48.).sin()
            })
            .collect()
    }
    #[test]
    fn arrival_recovers_offset_without_aligning_away_delay_and_refuses_clips() {
        let t = burst();
        let mut x = vec![0.00001; 8000];
        for (i, v) in t.iter().enumerate() {
            x[i + 1000] += v;
        }
        let a = arrival(&x, &t, [0, 5000], 0.00001).unwrap();
        assert!(a.frame.abs_diff(1000) <= 1);
        x[1200] = 0.9;
        assert!(
            arrival(&x, &t, [0, 5000], 0.00001)
                .unwrap_err()
                .contains("clipped")
        );
    }
    #[test]
    fn reflections_and_noise_refuse() {
        let t = burst();
        let mut x = vec![0.; 9000];
        for (i, v) in t.iter().enumerate() {
            x[i + 500] = *v;
            x[i + 4000] = *v;
        }
        assert!(
            arrival(&x, &t, [0, 8000], 0.00001)
                .unwrap_err()
                .contains("ambiguous")
        );
        assert!(
            arrival(&x, &t, [0, 3500], 0.01)
                .unwrap_err()
                .contains("15 dB")
        );
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpectrumRequest {
    /// At least three unchanged-position repeats. No per-capture normalization.
    pub before: Vec<PathBuf>,
    pub after: Vec<PathBuf>,
    pub noise: PathBuf,
    pub channel: usize,
    pub signal_window: [usize; 2],
    pub noise_window: [usize; 2],
    /// A recorded placement/gain/amp basis, compared exactly by the caller.
    pub unchanged_basis: String,
}
pub(crate) fn spectrum(x: &[f64]) -> Result<Vec<f64>, String> {
    const N: usize = 4096;
    if x.len() < N
        || x.iter()
            .any(|s| !s.is_finite() || s.abs() >= 10f64.powf(-3. / 20.))
    {
        return Err("spectrum short/clipped/nonfinite".into());
    }
    let mut power = vec![0.; N / 2 + 1];
    let mut count = 0.;
    let window: Vec<f64> = (0..N)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (N - 1) as f64).cos())
        .collect();
    let normalization = N as f64 * window.iter().map(|w| w * w).sum::<f64>();
    let mut fft = vec![(0., 0.); N];
    for start in (0..=x.len() - N).step_by(N / 2) {
        for i in 0..N {
            fft[i] = (x[start + i] * window[i], 0.);
        }
        crate::sweep::fft(&mut fft, false);
        for k in 0..=N / 2 {
            power[k] += (fft[k].0.powi(2) + fft[k].1.powi(2))
                * if k == 0 || k == N / 2 { 1. } else { 2. }
                / normalization;
        }
        count += 1.;
    }
    for p in &mut power {
        *p /= count;
    }
    Ok(power)
}
pub fn compare_spectra(r: SpectrumRequest) -> Result<serde_json::Value, String> {
    if !(3..=8).contains(&r.before.len())
        || r.after.len() != r.before.len()
        || r.unchanged_basis.is_empty()
        || r.unchanged_basis.len() > 2048
    {
        return Err("three to eight paired repeats and unchanged basis required".into());
    }
    let noise = wav(&r.noise, r.channel)?;
    let noise = spectrum(slice(&noise, r.noise_window)?)?;
    let mut sets = Vec::new();
    for paths in [&r.before, &r.after] {
        let mut spectra = Vec::new();
        for path in paths {
            let x = wav(path, r.channel)?;
            spectra.push(spectrum(slice(&x, r.signal_window)?)?);
        }
        sets.push(spectra);
    }
    let mut bands = Vec::new();
    for b in 0..22 {
        let center = 100. * 2f64.powf(b as f64 / 3.);
        let low = center / 2f64.powf(1. / 6.);
        let high = center * 2f64.powf(1. / 6.);
        if high > 12000. {
            break;
        }
        let range =
            ((low * 4096. / RATE).ceil() as usize)..=((high * 4096. / RATE).floor() as usize);
        let np = range.clone().map(|k| noise[k]).sum::<f64>().max(1e-20);
        let values: Vec<Vec<f64>> = sets
            .iter()
            .map(|set| {
                set.iter()
                    .map(|s| range.clone().map(|k| s[k]).sum::<f64>())
                    .collect()
            })
            .collect();
        let usable = values
            .iter()
            .flatten()
            .all(|p| *p / np >= 10f64.powf(15. / 10.));
        let means: Vec<f64> = values
            .iter()
            .map(|v| v.iter().sum::<f64>() / v.len() as f64)
            .collect();
        let ranges: Vec<Option<f64>> = values
            .iter()
            .map(|v| {
                let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
                let hi = v.iter().copied().fold(0f64, f64::max);
                (lo > 0.).then(|| 10. * (hi / lo).log10())
            })
            .collect();
        bands.push(serde_json::json!({"center_hz":center,"usable":usable,"noise_power_dbfs":10.*np.log10(),"before_power_dbfs":(means[0]>0.).then(||10.*means[0].log10()),"after_power_dbfs":(means[1]>0.).then(||10.*means[1].log10()),"after_minus_before_db":usable.then(||10.*(means[1]/means[0]).log10()),"repeat_range_db":ranges}));
    }
    Ok(
        serde_json::json!({"contract":"SHR-PA-ACOUSTIC-SPECTRA","version":1,"basis":r.unchanged_basis,"bands":bands,"normalization":"fixed Hann power, no per-file level normalization","calibration":null,"eq_proposal":null,"limits":"Comparative observations at supplied position/gain only. Weak bands excluded. Nominal microphone response is not individual calibration; no automated EQ or SPL."}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeakageRequest {
    pub capture: PathBuf,
    pub channel: usize,
    pub dry_window: [usize; 2],
    pub wet_window: [usize; 2],
    pub noise_window: [usize; 2],
    pub carrier_hz: f64,
}
fn carrier_power(x: &[f64], hz: f64) -> Result<f64, String> {
    if !(480..=4800).contains(&x.len())
        || x.iter()
            .any(|s| !s.is_finite() || s.abs() >= 10f64.powf(-3. / 20.))
    {
        return Err("carrier window bounds/clipping".into());
    }
    let mut re = 0.;
    let mut im = 0.;
    let mut weight = 0.;
    for (i, &v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (x.len() - 1) as f64).cos();
        let phase = std::f64::consts::TAU * hz * i as f64 / RATE;
        re += w * v * phase.cos();
        im += w * v * phase.sin();
        weight += w;
    }
    Ok(2. * (re * re + im * im) / (weight * weight))
}
pub fn leakage(r: LeakageRequest) -> Result<serde_json::Value, String> {
    if !r.carrier_hz.is_finite() || !(80. ..=12000.).contains(&r.carrier_hz) {
        return Err("carrier frequency bounds".into());
    }
    let x = wav(&r.capture, r.channel)?;
    let windows = [r.dry_window, r.wet_window, r.noise_window];
    let mut power = Vec::new();
    let len = r.dry_window[1]
        .checked_sub(r.dry_window[0])
        .ok_or("window order")?;
    for w in windows {
        let v = slice(&x, w)?;
        if v.len() != len {
            return Err("identical carrier windows required".into());
        }
        power.push(carrier_power(v, r.carrier_hz)?);
    }
    let noise = power[2].max(1e-20);
    let threshold = noise * 10f64.powf(15. / 10.);
    if power[1] < threshold {
        return Err("wet carrier below 15 dB background margin".into());
    }
    let detected = power[0] >= threshold;
    Ok(
        serde_json::json!({"contract":"SHR-PA-ACOUSTIC-LEAKAGE","version":1,"carrier_hz":r.carrier_hz,"dry_carrier_detected":detected,"dry_carrier_dbfs":detected.then(||10.*power[0].log10()),"dry_detection_threshold_dbfs":10.*threshold.log10(),"wet_carrier_dbfs":10.*power[1].log10(),"wet_over_dry_db":detected.then(||10.*(power[1]/power[0]).log10()),"wet_over_detection_threshold_db":10.*(power[1]/threshold).log10(),"digital_fader_floor_db":-60,"exact_dry_silence":false,"limits":"Below-threshold dry carrier is unresolved, not zero. Threshold is an empirical noise margin, not a confidence interval. Check bypass baseline for reflections and other carrier sources."}),
    )
}
#[cfg(test)]
mod spectral_tests {
    use super::*;
    #[test]
    fn fixed_spectrum_preserves_level_and_rejects_clipping() {
        let x: Vec<f64> = (0..8192)
            .map(|i| 0.01 * (std::f64::consts::TAU * 1000. * i as f64 / RATE).sin())
            .collect();
        let a = spectrum(&x).unwrap();
        let y: Vec<f64> = x.iter().map(|s| s * 10f64.powf(-3. / 20.)).collect();
        let b = spectrum(&y).unwrap();
        let ratio = b.iter().sum::<f64>() / a.iter().sum::<f64>();
        assert!((10. * ratio.log10() + 3.).abs() < 1e-10);
        assert!(spectrum(&vec![0.9; 8192]).is_err());
        assert!(spectrum(&[0.; 48]).is_err());
        let p = carrier_power(&x[..2400], 1000.).unwrap();
        assert!((10. * p.log10() - (-43.0103)).abs() < 0.01);
    }
}
