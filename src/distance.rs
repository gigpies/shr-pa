//! Presentation conversion only: added acoustic path equivalent at 343 m/s.
pub const SOUND_SPEED_M_S: f64 = 343.;
pub fn parse_ms(text: &str, maximum_ms: f64) -> Result<f64, &'static str> {
    let text = text.trim();
    let (number, scale) = if let Some(n) = text.strip_suffix("cm") {
        (n, 10. / SOUND_SPEED_M_S)
    } else if let Some(n) = text.strip_suffix("ms") {
        (n, 1.)
    } else if let Some(n) = text.strip_suffix('m') {
        (n, 1000. / SOUND_SPEED_M_S)
    } else {
        (text, 1.)
    };
    let ms = number
        .trim()
        .parse::<f64>()
        .map_err(|_| "delay number/unit")?
        * scale;
    if !ms.is_finite() || !maximum_ms.is_finite() || !(0.0..=maximum_ms).contains(&ms) {
        return Err("delay outside owner bounds");
    }
    Ok(ms)
}
pub fn applied(ms: f64, rate: u32) -> (u64, f64, f64) {
    let samples = (ms * f64::from(rate) / 1000.).round() as u64;
    let seconds = samples as f64 / f64::from(rate);
    (samples, seconds * 1000., seconds * SOUND_SPEED_M_S)
}
pub fn display(ms: f64, rate: u32) -> String {
    let (_, ms, metres) = applied(ms, rate);
    format!("{ms:.3}ms ~{metres:.2}m/{:.0}cm", metres * 100.)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn units_bounds_rounding_and_roundtrip() {
        for (text, expected) in [("0cm", 0.), ("3.43m", 10.), ("343cm", 10.), ("10 ms", 10.)] {
            assert!((parse_ms(text, 10.).unwrap() - expected).abs() < 1e-12);
        }
        for bad in ["NaN", "inf", "-1cm", "3.44m", "1mm", ""] {
            assert!(parse_ms(bad, 10.).is_err());
        }
        for rate in [8000, 44100, 48000, 192000] {
            for sample in [0, 1, 17, 80] {
                let ms = sample as f64 * 1000. / rate as f64;
                let (_, actual, m) = applied(ms, rate);
                assert!((parse_ms(&format!("{m}m"), 100.).unwrap() - actual).abs() < 1e-10);
                assert_eq!(applied(actual, rate).0, sample);
            }
        }
        assert_eq!(applied(0.01, 48000).0, 0);
        assert_eq!(applied(0.011, 48000).0, 1);
    }
}
