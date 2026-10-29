//! SHR-PA-CAL 1: bounded, magnitude-only correction, not absolute SPL calibration.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calibration {
    pub provenance: String,
    pub points: Vec<[f64; 2]>,
}
impl Calibration {
    /// Header then ascending `Hz correction_dB` pairs. Positive correction adds dB.
    pub fn parse(text: &str, provenance: &str) -> Result<Self, &'static str> {
        if text.len() > 32768
            || provenance.is_empty()
            || provenance.len() > 128
            || !provenance.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err("calibration_capacity_or_provenance");
        }
        let mut lines = text.lines();
        if lines.next() != Some("SHR-PA-CAL 1 Hz correction_dB") {
            return Err("calibration_header");
        }
        let mut points = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() != 2 {
                return Err("calibration_columns");
            }
            points.push([
                fields[0].parse().map_err(|_| "calibration_number")?,
                fields[1].parse().map_err(|_| "calibration_number")?,
            ]);
            if points.len() > 256 {
                return Err("calibration_capacity");
            }
        }
        let c = Self {
            provenance: provenance.into(),
            points,
        };
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(2..=256).contains(&self.points.len())
            || self.provenance.is_empty()
            || self.provenance.len() > 128
            || !self.provenance.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err("calibration_shape");
        }
        let mut previous = 0.;
        for &[hz, db] in &self.points {
            if !hz.is_finite()
                || !db.is_finite()
                || !(10.0..=24000.0).contains(&hz)
                || !(-20.0..=20.0).contains(&db)
                || hz <= previous
            {
                return Err("calibration_range_order");
            }
            previous = hz;
        }
        Ok(())
    }
    pub fn correction_db(&self, hz: f64) -> Result<f64, &'static str> {
        self.validate()?;
        if !hz.is_finite() || hz < self.points[0][0] || hz > self.points.last().unwrap()[0] {
            return Err("calibration_outside_coverage");
        }
        for pair in self.points.windows(2) {
            let [a, b] = [pair[0], pair[1]];
            if hz <= b[0] {
                return Ok(a[1] + (b[1] - a[1]) * (hz / a[0]).ln() / (b[0] / a[0]).ln());
            }
        }
        unreachable!()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_format_and_log_interpolation() {
        let c = Calibration::parse(
            "SHR-PA-CAL 1 Hz correction_dB\n100 -2\n10000 2\n",
            "invented",
        )
        .unwrap();
        assert!(c.correction_db(1000.).unwrap().abs() < 1e-12);
        assert!(c.correction_db(99.).is_err());
        for bad in [
            "100 0",
            "SHR-PA-CAL 1 Hz correction_dB\n100 0\n100 1",
            "SHR-PA-CAL 1 Hz correction_dB\n100 NaN\n1000 1",
        ] {
            assert!(Calibration::parse(bad, "x").is_err());
        }
    }
}
