//! Offline conservative fitter. Produces detached output-PEQ candidates only.
use crate::{
    config::{EqBand, EqKind},
    dsp::Biquad,
    graph::GraphConfig,
    measurement::{Measurement, counter},
};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Flat,
    BassShaped,
    DampedHighFrequency,
}
impl Target {
    pub fn db(self, hz: f64) -> f64 {
        match self {
            Self::Flat => 0.,
            Self::BassShaped => (3. * (200. / hz).log2()).clamp(0., 3.),
            Self::DampedHighFrequency => (-1.5 * (hz / 2000.).log2()).clamp(-6., 0.),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EqProposal {
    pub contract: String,
    pub version: u32,
    pub status: String,
    pub reason: String,
    pub basis_capture: crate::measurement::CaptureMeta,
    pub basis_configuration: GraphConfig,
    pub basis_configuration_revision: String,
    pub measurement_ids: Vec<String>,
    pub output_index: usize,
    pub target: Target,
    pub calibration: Option<crate::calibration::Calibration>,
    pub bands: Vec<EqBand>,
    pub before_rms_db: f64,
    pub predicted_rms_db: f64,
    pub required_headroom_db: f64,
    pub verification_required: bool,
}
/// Production coefficient response; independent render tests verify the objective.
fn response(band: EqBand, rate: u32, hz: f64) -> f64 {
    let [b0, b1, b2, a1, a2] = Biquad::eq(band, rate).coefficients();
    let w = 2. * PI * hz / f64::from(rate);
    let numerator =
        (b0 + b1 * w.cos() + b2 * (2. * w).cos()).hypot(-b1 * w.sin() - b2 * (2. * w).sin());
    let denominator =
        (1. + a1 * w.cos() + a2 * (2. * w).cos()).hypot(-a1 * w.sin() - a2 * (2. * w).sin());
    20. * (numerator / denominator).log10()
}
fn rms(values: &[f64], weights: &[f64]) -> f64 {
    (values
        .iter()
        .zip(weights)
        .map(|(v, w)| v * v * w)
        .sum::<f64>()
        / weights.iter().sum::<f64>())
    .sqrt()
}
/// Up to four sequential positions, equal log-magnitude weight, <=4 dB spread.
/// Only unused enabled output PEQ slots are filled; no existing correction is erased.
pub fn fit(
    base: &GraphConfig,
    revision: &str,
    positions: &[Measurement],
    target: Target,
) -> Result<EqProposal, &'static str> {
    base.validate()?;
    if counter(revision)? == 0
        || positions.is_empty()
        || positions.len() > 4
        || base.sample_rate != 48000
    {
        return Err("eq_basis_limits");
    }
    let first = &positions[0];
    let output = first.capture.output_index;
    let p = base.outputs.get(output).ok_or("eq_output")?;
    let mut result = EqProposal {
        contract: "C-PA-EQ-PROPOSAL".into(),
        version: 1,
        status: "refused".into(),
        reason: String::new(),
        basis_capture: first.capture.clone(),
        basis_configuration: base.clone(),
        basis_configuration_revision: revision.into(),
        measurement_ids: positions.iter().map(|m| m.capture.id.clone()).collect(),
        output_index: output,
        target,
        calibration: None,
        bands: vec![],
        before_rms_db: 0.,
        predicted_rms_db: 0.,
        required_headroom_db: 0.,
        verification_required: true,
    };
    let decision = (|| -> Result<(), &'static str> {
        if p.source.is_none() || !p.processing.eq_enabled {
            return Err("eq_bank_unavailable");
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut locations = std::collections::BTreeSet::new();
        for m in positions {
            crate::measurement::validate_transfer(m)?;
            if !crate::measurement::same_basis(&first.capture, &m.capture)
                || m.capture.configuration_revision != revision
                || m.capture.output_index != output
                || m.options != first.options
                || !ids.insert(&m.capture.id)
                || !locations.insert(&m.capture.position_id)
            {
                return Err("eq_incompatible_basis");
            }
        }
        let has_deep_null = positions
            .iter()
            .flat_map(|m| &m.spectrum)
            .any(|b| b.magnitude < 10_f64.powf(-12. / 20.));
        let mut hz = Vec::new();
        let mut error = Vec::new();
        for bin in &first.spectrum {
            if !(40.0..=16000.0).contains(&bin.frequency_hz) || bin.coherence < 0.98 {
                continue;
            }
            let mut db = Vec::new();
            for m in positions {
                let b = m
                    .spectrum
                    .iter()
                    .find(|b| b.frequency_hz == bin.frequency_hz && b.coherence >= 0.98)
                    .ok_or("eq_incomplete_positions")?;
                db.push(20. * b.magnitude.log10());
            }
            let low = db.iter().copied().fold(f64::INFINITY, f64::min);
            let high = db.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if high - low > 4. {
                return Err("eq_conflicting_positions");
            }
            // Deep nulls are excluded, never offered boost. No absolute SPL inference.
            let mean = db.iter().sum::<f64>() / db.len() as f64;
            if mean < -12. {
                continue;
            }
            hz.push(bin.frequency_hz);
            error.push(mean - target.db(bin.frequency_hz));
        }
        if hz.len() < 12 || hz.last().unwrap() / hz[0] < 4. {
            return Err("eq_insufficient_reliable_band");
        }
        // Equal log-frequency area, not equal linear-frequency bin count.
        let weights: Vec<_> = hz
            .iter()
            .enumerate()
            .map(|(i, &f)| {
                let lo = if i == 0 { f } else { (hz[i - 1] * f).sqrt() };
                let hi = if i + 1 == hz.len() {
                    f
                } else {
                    (hz[i + 1] * f).sqrt()
                };
                (hi / lo).ln()
            })
            .collect();
        result.before_rms_db = rms(&error, &weights);
        result.predicted_rms_db = result.before_rms_db;
        if result.before_rms_db <= 0.75 {
            result.status = "no_change".into();
            return Ok(());
        }
        let slots = p.processing.eq.iter().filter(|b| b.db == 0.).count();
        if slots == 0 {
            return Err("eq_no_unused_slots");
        }
        // Finite grid: at most 8 * 64 * 3 * 12 * 64 response evaluations.
        for _ in 0..slots.min(8) {
            let mut best = None;
            let mut score = rms(&error, &weights);
            for &f in &hz {
                if !(50.0..=12000.0).contains(&f) {
                    continue;
                }
                for q in [0.5, 1., 2.] {
                    for gain in [-6., -5., -4., -3., -2., -1., 1., 2., 3.] {
                        if has_deep_null && gain > 0. {
                            continue;
                        }
                        let band = EqBand {
                            kind: EqKind::Bell,
                            slope: 1.,
                            hz: f,
                            q,
                            db: gain,
                        };
                        let predicted: Vec<_> = hz
                            .iter()
                            .zip(&error)
                            .map(|(&h, &e)| e + response(band, base.sample_rate, h))
                            .collect();
                        let candidate = rms(&predicted, &weights);
                        // Never improve aggregate by harming one reliable bin > 1 dB.
                        if candidate + 0.05 < score
                            && predicted
                                .iter()
                                .zip(&error)
                                .all(|(a, b)| a.abs() <= b.abs() + 1.)
                        {
                            let boost = result.bands.iter().map(|b| b.db.max(0.)).sum::<f64>()
                                + gain.max(0.);
                            let cut = result.bands.iter().map(|b| b.db.min(0.)).sum::<f64>()
                                + gain.min(0.);
                            if boost <= 3. && cut >= -12. {
                                score = candidate;
                                best = Some((band, predicted));
                            }
                        }
                    }
                }
            }
            let Some((band, predicted)) = best else {
                break;
            };
            result.bands.push(band);
            error = predicted;
            result.predicted_rms_db = score;
            if score <= 0.75 {
                break;
            }
        }
        if result.bands.is_empty() || result.before_rms_db - result.predicted_rms_db < 0.25 {
            result.bands.clear();
            result.predicted_rms_db = result.before_rms_db;
            return Err("eq_no_safe_improvement");
        }
        result.required_headroom_db = result.bands.iter().map(|b| b.db.max(0.)).sum();
        result.status = "proposed".into();
        Ok(())
    })();
    result.reason = match decision {
        Ok(()) => "offline_prediction_requires_fresh_verification",
        Err(e) => e,
    }
    .into();
    Ok(result)
}
impl EqProposal {
    pub fn candidate_configuration(
        &self,
        current: &GraphConfig,
        revision: &str,
    ) -> Result<GraphConfig, &'static str> {
        if self.contract != "C-PA-EQ-PROPOSAL"
            || self.version != 1
            || self.status != "proposed"
            || !self.verification_required
            || &self.basis_configuration != current
            || self.basis_configuration_revision != revision
            || self.bands.is_empty()
            || self.bands.len() > 8
        {
            return Err("eq_stale_or_invalid_proposal");
        }
        current.validate()?;
        if let Some(calibration) = &self.calibration {
            calibration.validate()?;
        }
        crate::measurement::validate_meta(&self.basis_capture, 0)?;
        if self.basis_capture.configuration_revision != revision
            || self.basis_capture.output_index != self.output_index
            || self.measurement_ids.is_empty()
            || self.measurement_ids.len() > 4
            || self.measurement_ids.iter().any(|id| {
                id.is_empty() || id.len() > 128 || !id.bytes().all(|b| b.is_ascii_graphic())
            })
            || ![
                self.before_rms_db,
                self.predicted_rms_db,
                self.required_headroom_db,
            ]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.)
            || self.before_rms_db - self.predicted_rms_db < 0.25
            || self.required_headroom_db != self.bands.iter().map(|b| b.db.max(0.)).sum::<f64>()
        {
            return Err("eq_proposal_evidence");
        }
        let mut next = current.clone();
        let p = &mut next
            .outputs
            .get_mut(self.output_index)
            .ok_or("eq_output")?
            .processing;
        if !p.eq_enabled {
            return Err("eq_bank_unavailable");
        }
        let mut bands = self.bands.iter();
        for slot in &mut p.eq {
            if slot.db == 0.
                && let Some(b) = bands.next()
            {
                if b.kind != EqKind::Bell
                    || ![b.hz, b.db, b.q, b.slope].iter().all(|v| v.is_finite())
                    || !(50.0..=12000.0).contains(&b.hz)
                    || !(-6.0..=3.0).contains(&b.db)
                    || !(0.5..=2.0).contains(&b.q)
                    || b.slope != 1.
                {
                    return Err("eq_band_limits");
                }
                *slot = *b;
            }
        }
        if bands.next().is_some()
            || self.bands.iter().map(|b| b.db.max(0.)).sum::<f64>() > 3.
            || self.bands.iter().map(|b| b.db.min(0.)).sum::<f64>() < -12.
        {
            return Err("eq_bank_limits");
        }
        next.validate()?;
        Ok(next)
    }
}

/// Preserve calibration provenance when fitting qualified offline sweep ensembles.
pub fn fit_sweeps(
    base: &GraphConfig,
    revision: &str,
    positions: &[crate::sweep::SweepResult],
    target: Target,
) -> Result<EqProposal, &'static str> {
    if positions.is_empty() || positions.len() > 4 {
        return Err("eq_sweep_positions");
    }
    for p in positions {
        if p.contract != "C-PA-SWEEP-RESULT"
            || p.version != 1
            || p.calibration != positions[0].calibration
            || !p.reasons.is_empty()
            || p.impulse.len() != 4096
            || p.impulse.iter().any(|v| !v.is_finite())
            || p.transfer.algorithm != "offline-sweep-ensemble-v1"
        {
            return Err("eq_sweep_quality_or_calibration_conflict");
        }
        if let Some(calibration) = &p.calibration {
            calibration.validate()?;
            for bin in &p.transfer.spectrum {
                calibration.correction_db(bin.frequency_hz)?;
            }
        }
    }
    let mut result = fit(
        base,
        revision,
        &positions
            .iter()
            .map(|p| p.transfer.clone())
            .collect::<Vec<_>>(),
        target,
    )?;
    result.calibration = positions[0].calibration.clone();
    Ok(result)
}
