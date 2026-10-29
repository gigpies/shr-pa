//! Reproducible invented calibrated sweep input for cross-module compatibility tests.
//! Emits an owner eq_sweeps request; no PCM, playback or physical measurement.
use shr_pa::{
    calibration::Calibration,
    graph::GraphConfig,
    measurement::CaptureMeta,
    sweep::{SweepSpec, extract_repeated},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: offline-review-fixture GRAPH_JSON REVISION".into());
    }
    let configuration: GraphConfig = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    configuration.validate()?;
    let spec = SweepSpec {
        start_hz: 40.,
        end_hz: 16000.,
        sweep_samples: 16384,
        total_samples: 32768,
        amplitude: 0.03,
    };
    let reference = spec.excitation()?;
    let response: Vec<_> = (0..reference.len())
        .map(|i| if i >= 23 { reference[i - 23] * 0.5 } else { 0. })
        .collect();
    let calibration = Calibration::parse(
        "SHR-PA-CAL 1 Hz correction_dB\n20 6.020599913279624\n20000 6.020599913279624",
        "invented-half-gain-microphone",
    )?;
    let meta = CaptureMeta {
        id: "invented-offline-position".into(),
        source_epoch: "77".into(),
        map_revision: "88".into(),
        clock_domain: "invented-offline-common-origin".into(),
        reference_id: "invented-reference".into(),
        reference_tap: "pre-pa".into(),
        reference_offset_frames: "17".into(),
        first_frame: "10000".into(),
        sample_rate: 48000,
        output_index: 0,
        position_id: "invented-position-1".into(),
        configuration_revision: args[2].clone(),
        dropped_frames: "0".into(),
        clipped_reference: false,
        clipped_mic: false,
        timing_verified: true,
    };
    let sweep = extract_repeated(
        &meta,
        &spec,
        &reference,
        &vec![response; 4],
        Some(&calibration),
    )?;
    println!(
        "{}",
        serde_json::json!({"contract":"C-PA-MEASUREMENT","version":2,"operation":"eq_sweeps","configuration_revision":args[2],"configuration":configuration,"positions":[sweep],"target":"damped_high_frequency"})
    );
    Ok(())
}
