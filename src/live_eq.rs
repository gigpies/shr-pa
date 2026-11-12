//! Optional EQ-only transactions. These do not authorize or rearm an output.
use crate::{
    config::{Config, EqBand, GEQ_HZ},
    dsp::Biquad,
    graph::Input,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EqSettings {
    pub input_index: usize,
    pub eq_enabled: bool,
    pub eq: [EqBand; 8],
    pub geq_enabled: bool,
    pub geq_db: [f64; 31],
}
impl EqSettings {
    pub fn from_input(input_index: usize, input: &Input) -> Self {
        Self {
            input_index,
            eq_enabled: input.eq_enabled,
            eq: input.eq,
            geq_enabled: input.geq_enabled,
            geq_db: input.geq_db,
        }
    }
    pub(crate) fn install(&self, i: &mut Input) {
        i.eq_enabled = self.eq_enabled;
        i.eq = self.eq;
        i.geq_enabled = self.geq_enabled;
        i.geq_db = self.geq_db;
    }
    pub(crate) fn bank(&self, rate: u32) -> Result<Vec<Biquad>, &'static str> {
        let mut bank = Vec::with_capacity(39);
        for e in self.eq {
            bank.push(if self.eq_enabled {
                Biquad::eq(e, rate)
            } else {
                Biquad::identity()
            });
        }
        for (index, &hz) in GEQ_HZ.iter().enumerate() {
            bank.push(if self.geq_enabled && hz <= 0.45 * rate as f64 {
                Biquad::eq(
                    EqBand {
                        hz,
                        q: 4.318,
                        db: self.geq_db[index],
                        ..EqBand::default()
                    },
                    rate,
                )
            } else {
                Biquad::identity()
            });
        }
        if !bank.iter().all(Biquad::valid) {
            return Err("unstable/nonfinite EQ bank");
        }
        Ok(bank)
    }
    /// [b0,b1,b2,a1,a2]; denominator 1+a1*z^-1+a2*z^-2.
    pub fn coefficients(&self, rate: u32) -> Result<Vec<[f64; 5]>, &'static str> {
        self.validate(rate)?;
        Ok(self.bank(rate)?.iter().map(Biquad::coefficients).collect())
    }
    fn validate(&self, rate: u32) -> Result<(), &'static str> {
        let mut c = Config {
            sample_rate: rate,
            low_hz: 100.,
            high_hz: 1000.,
            ..Config::default()
        };
        c.input_eq = [self.eq; 2];
        c.geq.db = [self.geq_db; 2];
        c.validate()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EqPatch {
    pub version: u32,
    pub inputs: [EqSettings; 2],
}
impl EqPatch {
    pub fn parse(json: &[u8], rate: u32, count: usize) -> Result<Self, &'static str> {
        if json.is_empty() || json.len() > 32768 {
            return Err("EQ JSON size");
        }
        let p: Self = serde_json::from_slice(json).map_err(|_| "strict EQ JSON")?;
        p.validate(rate, count)?;
        Ok(p)
    }
    pub fn validate(&self, rate: u32, count: usize) -> Result<(), &'static str> {
        if self.version != 1
            || self.inputs[0].input_index == self.inputs[1].input_index
            || self.inputs.iter().any(|e| e.input_index >= count)
        {
            return Err("EQ version/indices");
        }
        for e in &self.inputs {
            e.validate(rate)?;
        }
        Ok(())
    }
}
pub struct PreparedEq {
    pub(crate) previous: [EqSettings; 2],
    pub(crate) graph_generation: u64,
    pub(crate) epoch: u64,
    pub(crate) frame: u64,
    pub(crate) patch: EqPatch,
    pub(crate) banks: [Vec<Biquad>; 2],
    pub(crate) instance: u64,
    pub(crate) base_generation: u64,
    pub(crate) generation: u64,
    pub(crate) elapsed: u64,
    pub(crate) duration: u64,
    pub(crate) finished: bool,
    pub(crate) noop: bool,
    pub(crate) changed: [bool; 2],
}
impl PreparedEq {
    pub(crate) fn overlaps(&self, span: (usize, usize)) -> bool {
        let a = self as *const Self as usize;
        (a < span.1 && span.0 < a + std::mem::size_of::<Self>())
            || self.banks.iter().any(|v| {
                let a = v.as_ptr() as usize;
                a < span.1 && span.0 < a + v.capacity() * std::mem::size_of::<Biquad>()
            })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct EqProgress {
    pub generation: u64,
    pub remaining: u64,
    pub retirement_occupied: bool,
    pub eligible: bool,
}

/// Coefficient construction from a copied owner context, with no active graph access.
pub(crate) fn prepare_detached(
    patch: EqPatch,
    previous: [EqSettings; 2],
    rate: u32,
    count: usize,
    instance: u64,
    base_generation: u64,
) -> Result<Box<PreparedEq>, &'static str> {
    patch.validate(rate, count)?;
    let previous = patch.inputs.each_ref().map(|e| {
        previous
            .iter()
            .find(|p| p.input_index == e.input_index)
            .cloned()
            .ok_or("EQ context input identity")
    });
    let [a, b] = previous;
    let previous = [a?, b?];
    let generation = base_generation
        .checked_add(1)
        .ok_or("EQ generation exhausted")?;
    let banks = [patch.inputs[0].bank(rate)?, patch.inputs[1].bank(rate)?];
    let changed = [
        patch.inputs[0] != previous[0],
        patch.inputs[1] != previous[1],
    ];
    let noop = !changed[0] && !changed[1];
    Ok(Box::new(PreparedEq {
        previous,
        graph_generation: 0,
        epoch: 0,
        frame: 0,
        patch,
        banks,
        instance,
        base_generation,
        generation,
        elapsed: 0,
        duration: (rate as u64).div_ceil(200).max(2),
        finished: noop,
        noop,
        changed,
    }))
}
