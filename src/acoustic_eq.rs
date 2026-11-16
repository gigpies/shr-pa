//! Single-position magnitude-only proposals for independent-clock USB benches.
//! No coherence, common origin, phase, calibration or GP20 metadata is invented.
use crate::{
    acoustic::{slice, spectrum, wav},
    auto_eq::{Target, response},
    config::{EqBand, EqKind},
    graph::GraphConfig,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub capture: PathBuf,
    pub channel: usize,
    pub repeats: Vec<[usize; 2]>,
    pub noise: [usize; 2],
    pub reference: PathBuf,
    pub reference_channel: usize,
    pub reference_window: [usize; 2],
    pub basis: String,
    pub configuration: GraphConfig,
    pub configuration_revision: String,
    pub output_index: usize,
    pub target: Target,
}

// Broad cuts only; use the owner's actual DSP coefficient response. A candidate
// must improve the declared objective without making any reliable band >1dB worse.
fn fit(hz: &[f64], errors: &[f64], slots: usize) -> (Vec<EqBand>, Vec<f64>) {
    let mut residual = errors.to_vec();
    let mut bands: Vec<EqBand> = Vec::new();
    let score = |x: &[f64]| x.iter().map(|v| v * v).sum::<f64>();
    for _ in 0..slots.min(2) {
        let mut best = None;
        let mut best_score = score(&residual);
        for &center in hz {
            if !(250. ..=6300.).contains(&center) {
                continue;
            }
            // Require three adjacent reliable third-octave bins above target;
            // an isolated peak or a gap in reliable evidence cannot select a cut.
            let neighborhood: Vec<_> = hz
                .iter()
                .zip(&residual)
                .filter(|(f, _)| (**f / center).log2().abs() < 0.34)
                .collect();
            if neighborhood.len() < 3 || neighborhood.iter().any(|(_, e)| **e < 1.) {
                continue;
            }
            for q in [0.5, 0.707] {
                for db in [-1., -2., -3.] {
                    if bands.iter().map(|b| b.db).sum::<f64>() + db < -6. {
                        continue;
                    }
                    let b = EqBand {
                        kind: EqKind::Bell,
                        hz: center,
                        q,
                        db,
                        slope: 1.,
                    };
                    let predicted: Vec<_> = hz
                        .iter()
                        .zip(&residual)
                        .map(|(&f, &e)| e + response(b, 48000, f))
                        .collect();
                    let value = score(&predicted);
                    if value + 0.1 < best_score
                        && predicted
                            .iter()
                            .zip(&residual)
                            .all(|(a, b)| a.abs() <= b.abs() + 1.)
                    {
                        best_score = value;
                        best = Some((b, predicted));
                    }
                }
            }
        }
        let Some((band, next)) = best else { break };
        bands.push(band);
        residual = next;
    }
    (bands, residual)
}

pub fn execute(r: Request) -> Result<serde_json::Value, String> {
    r.configuration.validate()?;
    crate::measurement::counter(&r.configuration_revision)?;
    if r.configuration.sample_rate != 48000
        || !(3..=8).contains(&r.repeats.len())
        || r.basis.is_empty()
        || r.basis.len() > 2048
    {
        return Err("bounded repeats, 48k configuration and placement/gain basis required".into());
    }
    let output = r
        .configuration
        .outputs
        .get(r.output_index)
        .ok_or("output index")?;
    if output.source.is_none() || !output.processing.eq_enabled {
        return Err("output EQ unavailable".into());
    }
    let capture = wav(&r.capture, r.channel)?;
    let reference = wav(&r.reference, r.reference_channel)?;
    let reference = spectrum(slice(&reference, r.reference_window)?)?;
    let noise = spectrum(slice(&capture, r.noise)?)?;
    let mut spectra = Vec::new();
    let mut end = 0;
    for &w in &r.repeats {
        if w[0] < end
            || w[1].saturating_sub(w[0]) < 48000
            || (w[0] < r.noise[1] && w[1] > r.noise[0])
        {
            return Err("nonoverlapping ordered >=1s repeats required".into());
        }
        spectra.push(spectrum(slice(&capture, w)?)?);
        end = w[1];
    }
    let mut observations = Vec::new();
    let mut reliable = Vec::new();
    for b in 0..19 {
        let hz = 125. * 2f64.powf(b as f64 / 3.);
        let lo = (hz / 2f64.powf(1. / 6.) * 4096. / 48000.).ceil() as usize;
        let hi = (hz * 2f64.powf(1. / 6.) * 4096. / 48000.).floor() as usize;
        let sum = |s: &[f64]| s[lo..=hi].iter().sum::<f64>().max(1e-20);
        let np = sum(&noise);
        let rp = sum(&reference);
        let db: Vec<_> = spectra
            .iter()
            .map(|s| 10. * (sum(s) / rp).log10())
            .collect();
        let snr = spectra
            .iter()
            .map(|s| 10. * (sum(s) / np).log10())
            .fold(f64::INFINITY, f64::min);
        let spread = db.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - db.iter().copied().fold(f64::INFINITY, f64::min);
        let mean = db.iter().sum::<f64>() / db.len() as f64;
        let usable = snr >= 15. && spread <= 1.5 && rp > 1e-12;
        observations.push(serde_json::json!({"hz":hz,"response_db":mean,"minimum_snr_db":snr,"repeat_range_db":spread,"usable":usable}));
        if usable {
            reliable.push((hz, mean - r.target.db(hz)));
        }
    }
    if reliable.len() < 12 || reliable.last().unwrap().0 / reliable[0].0 < 8. {
        return Err("insufficient reliable broadband evidence (12 bands / 3 octaves)".into());
    }
    let mut levels: Vec<_> = reliable.iter().map(|v| v.1).collect();
    levels.sort_by(f64::total_cmp);
    let offset = levels[levels.len() / 2];
    // Target level is the median measured transfer, not SPL or normalization of
    // individual captures. Deep deficits never select boosts or set target level.
    let hz: Vec<_> = reliable.iter().map(|v| v.0).collect();
    let errors: Vec<_> = reliable.iter().map(|v| v.1 - offset).collect();
    let slots = output.processing.eq.iter().filter(|b| b.db == 0.).count();
    let (mut bands, mut residual) = fit(&hz, &errors, slots);
    let rms = |x: &[f64]| (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt();
    if rms(&errors) - rms(&residual) < 0.25 {
        bands.clear();
        residual = errors.clone();
    }
    let mut candidate = r.configuration.clone();
    for (slot, band) in candidate.outputs[r.output_index]
        .processing
        .eq
        .iter_mut()
        .filter(|b| b.db == 0.)
        .zip(&bands)
    {
        *slot = *band;
    }
    candidate.validate()?;
    Ok(
        serde_json::json!({"contract":"SHR-PA-INDEPENDENT-CLOCK-EQ","version":1,"status":if bands.is_empty(){"no_change"}else{"proposed"},"basis":r.basis,"basis_configuration":r.configuration,"basis_configuration_revision":r.configuration_revision,"output_index":r.output_index,"target":r.target,"target_offset_db":offset,"bands":bands,"observations":observations,"before_rms_db":rms(&errors),"predicted_rms_db":rms(&residual),"candidate_configuration":candidate,"verification_required":true,"microphone_calibration":null,"common_clock":false,"limits":"Single-position uncalibrated magnitude shape includes microphone coloration and room uncertainty. Fixed Hann powers, >=15dB SNR, <=1.5dB repeat range. Median-relative target; maximum two broad Q<=0.707 bell cuts, <=3dB each / 6dB aggregate; no boosts. Not synchronized GP20, phase, SPL, absolute delay or multi-position qualification. Explicit muted review/apply/rearm and fresh remeasurement required."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_wav_proposal_preserves_other_processing_and_refuses_bad_evidence() {
        let dir = std::env::temp_dir().join(format!("shr-pa-acoustic-eq-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let mut seed = 17u32;
        let source: Vec<_> = (0..96000)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (f64::from(seed) / f64::from(u32::MAX) - 0.5) * 0.02
            })
            .collect();
        let mut filter = crate::dsp::Biquad::eq(
            EqBand {
                kind: EqKind::Bell,
                hz: 1000.,
                q: 0.707,
                db: 6.,
                slope: 1.,
            },
            48000,
        );
        let response: Vec<_> = source.iter().map(|&v| filter.tick(v)).collect();
        let mut capture = vec![0.00001; 48000];
        for _ in 0..3 {
            capture.extend_from_slice(&response);
        }
        let write = |name: &str, x: &[f64]| {
            let mut w = hound::WavWriter::create(
                dir.join(name),
                hound::WavSpec {
                    channels: 1,
                    sample_rate: 48000,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                },
            )
            .unwrap();
            for &v in x {
                w.write_sample(v as f32).unwrap();
            }
            w.finalize().unwrap();
        };
        write("capture.wav", &capture);
        write("reference.wav", &source);
        let request = || Request {
            capture: dir.join("capture.wav"),
            channel: 0,
            repeats: vec![[48000, 144000], [144000, 240000], [240000, 336000]],
            noise: [0, 48000],
            reference: dir.join("reference.wav"),
            reference_channel: 0,
            reference_window: [0, 96000],
            basis: "fixed synthetic regression".into(),
            configuration: GraphConfig::stereo(vec![]),
            configuration_revision: "1".into(),
            output_index: 0,
            target: Target::Flat,
        };
        let result = execute(request()).unwrap();
        assert_eq!(result["status"], "proposed");
        assert_eq!(result["common_clock"], false);
        let mut candidate: GraphConfig =
            serde_json::from_value(result["candidate_configuration"].clone()).unwrap();
        candidate.outputs[0].processing.eq = request().configuration.outputs[0].processing.eq;
        assert_eq!(candidate, request().configuration);
        let mut bad = request();
        bad.repeats[1] = bad.repeats[0];
        assert!(execute(bad).is_err());
        capture[50000] = 0.9;
        write("capture.wav", &capture);
        assert!(execute(request()).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn broad_excess_gets_bounded_cut_but_null_and_isolated_peak_do_not() {
        let hz: Vec<_> = (0..19).map(|i| 125. * 2f64.powf(i as f64 / 3.)).collect();
        let peak = EqBand {
            kind: EqKind::Bell,
            hz: 1000.,
            q: 0.707,
            db: 4.,
            slope: 1.,
        };
        let error: Vec<_> = hz.iter().map(|&f| response(peak, 48000, f)).collect();
        let (b, r) = fit(&hz, &error, 8);
        assert!(!b.is_empty());
        assert!(b.len() <= 2);
        assert!(b.iter().all(|b| b.db < 0. && b.db >= -3. && b.q <= 0.707));
        assert!(r.iter().map(|v| v * v).sum::<f64>() < error.iter().map(|v| v * v).sum());
        assert!(fit(&hz, &vec![-8.; hz.len()], 8).0.is_empty());
        let mut spike = vec![0.; hz.len()];
        spike[9] = 8.;
        assert!(fit(&hz, &spike, 8).0.is_empty());
        assert!(fit(&hz, &error, 0).0.is_empty());
    }
}
