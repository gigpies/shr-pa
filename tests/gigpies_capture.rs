//! Consumer check of the GigPies GP-PA-MEASURE-CAPTURE:1 corpus: this owner's sweep
//! extraction accepts the producer's capture metadata and specification, and its
//! generator reproduces the producer's excitation samples.
use shr_pa::{
    measurement::CaptureMeta,
    sweep::{SweepSpec, extract, extract_repeated},
};
use std::path::PathBuf;

const DIR: &str = "tests/fixtures/gigpies-capture/v1";

fn read(name: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(DIR).join(name)).unwrap()
}

fn record(name: &str) -> (CaptureMeta, SweepSpec, u64) {
    let v: serde_json::Value = serde_json::from_slice(&read(name)).unwrap();
    assert_eq!(v["contract"], "GP-PA-MEASURE-CAPTURE");
    assert_eq!(v["version"], 1);
    assert_eq!(v["outcome"], "captured");
    assert_eq!(v["restored"], true);
    let meta: CaptureMeta = serde_json::from_value(v["capture"].clone()).unwrap();
    let spec: SweepSpec = serde_json::from_value(v["owner_spec"].clone()).unwrap();
    (meta, spec, v["repeats"].as_u64().unwrap())
}

/// Known path: half gain, 23 samples of arrival, inside the analysis window.
fn response(reference: &[f64]) -> Vec<f64> {
    let mut out = vec![0.; reference.len()];
    out[23..].copy_from_slice(&reference[..reference.len() - 23]);
    out.iter_mut().for_each(|v| *v *= 0.5);
    out
}

#[test]
fn producer_samples_match_this_generator() {
    let v: serde_json::Value = serde_json::from_slice(&read("excitation-samples.json")).unwrap();
    let spec: SweepSpec = serde_json::from_value(v["spec"].clone()).unwrap();
    let excitation = spec.excitation().unwrap();
    let indexes = v["indexes"].as_array().unwrap();
    let values = v["values"].as_array().unwrap();
    assert_eq!(indexes.len(), values.len());
    for (i, value) in indexes.iter().zip(values) {
        let i = i.as_u64().unwrap() as usize;
        let value = value.as_f64().unwrap();
        // Well inside the 1e-10 reference tolerance of `extract`.
        assert!((excitation[i] - value).abs() <= 1e-15, "{i}");
    }
}

#[test]
fn repeated_record_feeds_sweep_repeated() {
    let (meta, spec, repeats) = record("record-captured.json");
    assert_eq!(repeats, 4);
    let reference = spec.excitation().unwrap();
    let responses = vec![response(&reference); 4];
    let result = extract_repeated(&meta, &spec, &reference, &responses, None).unwrap();
    assert_eq!(result.transfer.arrival_samples, 23);
    assert_eq!(result.transfer.capture, meta);
    assert!(result.transfer.spectrum.len() >= 12);
    assert!(result.transfer.spectrum.iter().all(|b| b.coherence >= 0.98));
}

#[test]
fn single_record_feeds_sweep() {
    let (meta, spec, repeats) = record("record-single.json");
    assert_eq!(repeats, 1);
    let reference = spec.excitation().unwrap();
    let result = extract(&meta, &spec, &reference, &response(&reference), None).unwrap();
    assert_eq!(result.transfer.arrival_samples, 23);
    assert!(
        result
            .reasons
            .contains(&"single_sweep_coherence_unavailable".into())
    );
}
