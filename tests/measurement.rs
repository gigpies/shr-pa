use shr_pa::{
    graph::{Graph, GraphConfig},
    measurement::*,
};
use std::f64::consts::PI;
fn meta(id: &str, output: usize) -> CaptureMeta {
    CaptureMeta {
        id: id.into(),
        source_epoch: "1".into(),
        map_revision: "1".into(),
        clock_domain: "synthetic-common-clock".into(),
        reference_id: "actual-input".into(),
        reference_tap: "program-pre-pa".into(),
        reference_offset_frames: "0".into(),
        first_frame: "10000".into(),
        sample_rate: 48000,
        output_index: output,
        position_id: "position-1".into(),
        configuration_revision: "1".into(),
        dropped_frames: "0".into(),
        clipped_reference: false,
        clipped_mic: false,
        timing_verified: true,
    }
}
fn noise() -> Vec<f64> {
    let mut state = 17_u64;
    (0..MIN_SAMPLES)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state as i64 as f64 / i64::MAX as f64) * 0.03
        })
        .collect()
}
fn shifted(x: &[f64], delay: usize, sign: f64) -> Vec<f64> {
    (0..x.len())
        .map(|i| if i >= delay { x[i - delay] * sign } else { 0. })
        .collect()
}
fn measure(id: &str, index: usize, x: &[f64], y: &[f64]) -> Measurement {
    analyze(&meta(id, index), x, y, &Options::default()).unwrap()
}
fn config() -> GraphConfig {
    let mut c = GraphConfig::stereo(vec![]);
    // Both independently protected outputs receive the same actual excitation.
    c.outputs[1].source = c.outputs[0].source;
    c
}
fn render(config: GraphConfig, x: &[f64]) -> Vec<Vec<f64>> {
    let mut graph = Graph::prepare(config.clone()).unwrap();
    graph.rearm().unwrap();
    graph
        .process(
            &vec![0.; config.inputs.len() * 256],
            &mut vec![0.; config.outputs.len() * 256],
        )
        .unwrap();
    let mut out = vec![Vec::new(); config.outputs.len()];
    for block in x.chunks(256) {
        let input: Vec<_> = block
            .iter()
            .flat_map(|&v| vec![v; config.inputs.len()])
            .collect();
        let mut output = vec![0.; block.len() * config.outputs.len()];
        graph.process(&input, &mut output).unwrap();
        for frame in output.chunks(config.outputs.len()) {
            for (c, &v) in frame.iter().enumerate() {
                out[c].push(v);
            }
        }
    }
    out
}
#[test]
fn production_graph_arrival_polarity_proposal_and_independent_sum() {
    let x = noise();
    let mut c = config();
    c.outputs[1].processing.delay_ms = 0.5;
    c.outputs[1].processing.inverted = true;
    let before = render(c.clone(), &x);
    let a = measure("a", 0, &x, &before[0]);
    let b = measure("b", 1, &x, &before[1]);
    assert_eq!(a.arrival_samples, 0);
    assert_eq!(b.arrival_samples, 24);
    for bin in &b.spectrum {
        let expected = PI - 2. * PI * bin.frequency_hz * 24. / 48000.;
        assert!((bin.magnitude - 1.).abs() < 1e-6);
        let error = bin.phase_radians - expected;
        assert!(error.sin().atan2(error.cos()).abs() < 1e-6);
        assert!(bin.coherence > 0.999999);
    }
    assert!(a.proposal_eligible, "{:?}", a.reasons);
    assert!(b.proposal_eligible, "{:?}", b.reasons);
    let proposal = propose(
        &c,
        "1",
        &[PositionPair {
            anchor: a,
            target: b,
        }],
    )
    .unwrap();
    assert_eq!(proposal.status, "proposed", "{}", proposal.reason);
    let next = proposal.candidate_configuration(&c, "1").unwrap();
    assert_eq!(next.outputs[0].processing.delay_ms, 0.5);
    assert!(!next.outputs[1].processing.inverted);
    let mut expected = c.clone();
    expected.outputs[0].processing.delay_ms = 0.5;
    expected.outputs[1].processing.inverted = false;
    assert_eq!(next, expected);
    let after = render(next, &x);
    let ideal = shifted(&x, 24, 2.);
    let mse = after[0]
        .iter()
        .zip(&after[1])
        .zip(&ideal)
        .map(|((&a, &b), &v)| (a + b - v).powi(2))
        .sum::<f64>()
        / x.len() as f64;
    assert!(mse < 1e-20, "{mse}");
    let before_energy = before[0]
        .iter()
        .zip(&before[1])
        .map(|(&a, &b)| (a + b).powi(2))
        .sum::<f64>();
    let after_energy = after[0]
        .iter()
        .zip(&after[1])
        .map(|(&a, &b)| (a + b).powi(2))
        .sum::<f64>();
    assert!(after_energy > before_energy * 1.8);
    assert!(proposal.candidate_configuration(&c, "2").is_err());
}
#[test]
fn zero_one_maximum_and_out_of_range_proposals() {
    let x = noise();
    let a = measure("a", 0, &x, &x);
    for delay in [0, 1, 480, 481] {
        let b = measure("b", 1, &x, &shifted(&x, delay, 1.));
        assert_eq!(b.arrival_samples, delay as i32);
        let p = propose(
            &config(),
            "1",
            &[PositionPair {
                anchor: a.clone(),
                target: b,
            }],
        )
        .unwrap();
        assert_eq!(
            p.status,
            if delay == 0 {
                "no_change"
            } else if delay <= 480 {
                "proposed"
            } else {
                "refused"
            },
            "{}",
            p.reason
        );
        if delay == 0 {
            assert_eq!(p.candidate_configuration(&config(), "1").unwrap(), config());
        }
    }
}
#[test]
fn timing_clipping_missing_and_schema_refusals() {
    let x = noise();
    for field in 0..4 {
        let mut m = meta("x", 0);
        match field {
            0 => m.dropped_frames = "1".into(),
            1 => m.clipped_mic = true,
            2 => m.timing_verified = false,
            _ => m.source_epoch = "01".into(),
        };
        assert!(analyze(&m, &x, &x, &Options::default()).is_err());
    }
    for bad in [f64::NAN, f64::INFINITY, 1., -1.] {
        let mut y = x.clone();
        y[100] = bad;
        assert!(analyze(&meta("x", 0), &x, &y, &Options::default()).is_err());
    }
    assert!(analyze(&meta("x", 0), &x[..100], &x[..100], &Options::default()).is_err());
    assert!(
        analyze(
            &meta("x", 0),
            &vec![0.; MIN_SAMPLES],
            &x,
            &Options::default()
        )
        .is_err()
    );
    let mut overflow = meta("overflow", 0);
    overflow.first_frame = u64::MAX.to_string();
    assert!(analyze(&overflow, &x, &x, &Options::default()).is_err());
    assert!(decode_request(br#"{"operation":"analyze","operation":"propose"}"#).is_err());
    assert!(decode_request(&vec![b' '; MAX_JSON_BYTES + 1]).is_err());
}
#[test]
fn noise_periodicity_and_reflections_withhold_proposals() {
    let x = noise();
    let periodic: Vec<_> = (0..MIN_SAMPLES)
        .map(|i| (2. * PI * i as f64 / 48.).sin() * 0.03)
        .collect();
    let p = measure("periodic", 0, &periodic, &periodic);
    assert!(!p.proposal_eligible, "{:?}", p.reasons);
    let reflected: Vec<_> = x
        .iter()
        .zip(shifted(&x, 100, 0.8))
        .map(|(&a, b)| a + b)
        .collect();
    let r = measure("reflection", 0, &x, &reflected);
    assert!(!r.proposal_eligible);
    let unrelated: Vec<_> = x.iter().rev().copied().collect();
    let n = measure("noise", 0, &x, &unrelated);
    assert!(!n.proposal_eligible);
}
#[test]
fn multiple_positions_require_exact_consensus_and_basis() {
    let x = noise();
    let a = measure("a", 0, &x, &x);
    let b = measure("b", 1, &x, &shifted(&x, 24, 1.));
    let first = PositionPair {
        anchor: a.clone(),
        target: b.clone(),
    };
    let mut second = first.clone();
    second.anchor.capture.id = "c".into();
    second.target.capture.id = "d".into();
    second.anchor.capture.position_id = "position-2".into();
    second.target.capture.position_id = "position-2".into();
    assert_eq!(
        propose(&config(), "1", &[first.clone(), second.clone()])
            .unwrap()
            .status,
        "proposed"
    );
    second.target = measure("d", 1, &x, &shifted(&x, 25, 1.));
    second.target.capture.position_id = "position-2".into();
    assert_eq!(
        propose(&config(), "1", &[first.clone(), second])
            .unwrap()
            .reason,
        "conflicting_positions"
    );
    let mut mismatch = first;
    mismatch.target.capture.source_epoch = "2".into();
    assert_eq!(
        propose(&config(), "1", &[mismatch]).unwrap().status,
        "refused"
    );
}
#[test]
fn production_crossover_transfer_is_complex_and_not_forced_into_delay() {
    let x = noise();
    let c = GraphConfig::stereo(vec![1000.]);
    let output = render(c, &x);
    let m = measure("high", 0, &x, &output[0]);
    assert!(m.spectrum.len() >= 8);
    assert!(m.spectrum.iter().any(|b| b.phase_radians.abs() > 0.1));
    // Independent bilinear analog LR24 high-pass reference, no owner coefficient reuse.
    for b in m.spectrum.iter().filter(|b| b.frequency_hz > 400.) {
        let k = (PI * b.frequency_hz / 48000.).tan() / (PI * 1000. / 48000.).tan();
        let expected_magnitude = k.powi(4) / ((1. - k * k).powi(2) + 2. * k * k);
        let expected_phase = 2. * (PI - (2_f64.sqrt() * k).atan2(1. - k * k));
        assert!(
            (b.magnitude - expected_magnitude).abs() < 0.015,
            "{}",
            b.frequency_hz
        );
        let error = b.phase_radians - expected_phase;
        assert!(
            error.sin().atan2(error.cos()).abs() < 0.03,
            "{}",
            b.frequency_hz
        );
    }
    // Independently measured production low/high responses have designed phase;
    // a dominant correlation peak is not permission to erase crossover behavior.
    let low = measure("low", 2, &x, &output[2]);
    let p = propose(
        &GraphConfig::stereo(vec![1000.]),
        "1",
        &[PositionPair {
            anchor: m,
            target: low,
        }],
    )
    .unwrap();
    assert_eq!(p.status, "refused");
}
#[test]
fn standalone_json_refusal_never_applies_or_opens_devices() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_shr-pa-measure"))
        .args(["unexpected", "extra"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "refused");
}
#[test]
fn clipping_range_rounding_and_proposal_tampering_preserve_base() {
    let x = noise();
    let a = measure("a", 0, &x, &x);
    let b = measure("b", 1, &x, &shifted(&x, 1, 1.));
    let mut c = config();
    c.outputs[0].processing.delay_ms = 9.999;
    let p = propose(
        &c,
        "1",
        &[PositionPair {
            anchor: a.clone(),
            target: b.clone(),
        }],
    )
    .unwrap();
    assert_eq!(p.reason, "output_delay_range");
    assert!(p.changes.is_empty());
    let mut p = propose(
        &config(),
        "1",
        &[PositionPair {
            anchor: a,
            target: b,
        }],
    )
    .unwrap();
    p.status = "no_change".into();
    assert!(p.candidate_configuration(&config(), "1").is_err());
    p.status = "proposed".into();
    p.added_latency_samples = 123;
    assert!(p.candidate_configuration(&config(), "1").is_err());
}
#[test]
fn negative_arrival_and_boundary_are_explicit() {
    let x = noise();
    let reference = shifted(&x, 24, 1.);
    let m = measure("early", 0, &reference, &x);
    assert_eq!(m.arrival_samples, -24);
    assert!(m.proposal_eligible);
    let options = Options {
        max_arrival_samples: 24,
        ..Options::default()
    };
    let boundary = analyze(&meta("edge", 0), &x, &shifted(&x, 24, 1.), &options).unwrap();
    assert!(!boundary.proposal_eligible);
    assert!(
        boundary
            .reasons
            .iter()
            .any(|r| r == "arrival_at_search_boundary")
    );
}
#[test]
fn bounded_json_accepts_real_request_and_rejects_unknown_and_nested_duplicates() {
    let request = serde_json::json!({"contract":"C-PA-MEASUREMENT","version":1,"operation":"analyze","capture":meta("x",0),"options":Options::default(),"reference":[],"mic":[]});
    let encoded = serde_json::to_vec(&request).unwrap();
    assert!(decode_request(&encoded).is_ok());
    let mut extra = request.clone();
    extra["surprise"] = true.into();
    assert!(decode_request(&serde_json::to_vec(&extra).unwrap()).is_err());
    let text = String::from_utf8(encoded).unwrap().replace(
        "\"sample_rate\":48000",
        "\"sample_rate\":48000,\"sample_rate\":48000",
    );
    assert!(decode_request(text.as_bytes()).is_err());
    let mut larger = request;
    larger["reference"] = serde_json::json!(vec![0; MAX_SAMPLES + 1]);
    assert!(decode_request(&serde_json::to_vec(&larger).unwrap()).is_err());
}
#[test]
fn target_earlier_inverted_and_output_map_drift() {
    let x = noise();
    let mut c = config();
    c.outputs[0].processing.delay_ms = 0.5;
    c.outputs[1].processing.inverted = true;
    let output = render(c.clone(), &x);
    let a = measure("a", 0, &x, &output[0]);
    let b = measure("b", 1, &x, &output[1]);
    let pair = PositionPair {
        anchor: a,
        target: b,
    };
    let p = propose(&c, "1", std::slice::from_ref(&pair)).unwrap();
    let next = p.candidate_configuration(&c, "1").unwrap();
    assert_eq!(next.outputs[1].processing.delay_ms, 0.5);
    assert!(!next.outputs[1].processing.inverted);
    let mut drift = c.clone();
    drift.outputs[1].source = Some(shr_pa::graph::Source::Input(1));
    assert!(p.candidate_configuration(&drift, "1").is_err());
    let mut mismatch = pair.clone();
    mismatch.target.capture.map_revision = "2".into();
    assert_eq!(
        propose(&c, "1", &[mismatch]).unwrap().reason,
        "incompatible_measurement_basis"
    );
    let mut offset_mismatch = pair.clone();
    offset_mismatch.target.capture.reference_offset_frames = "1".into();
    assert_eq!(
        propose(&c, "1", &[offset_mismatch]).unwrap().reason,
        "incompatible_measurement_basis"
    );
    let mut mismatch = pair;
    mismatch.target.capture.configuration_revision = "2".into();
    assert_eq!(
        propose(&c, "1", &[mismatch]).unwrap().reason,
        "incompatible_measurement_basis"
    );
}
#[test]
fn golden_owner_results_and_proposal_are_strictly_interoperable() {
    let a: Measurement =
        serde_json::from_str(include_str!("fixtures/measurement/v1/anchor-result.json")).unwrap();
    let b: Measurement =
        serde_json::from_str(include_str!("fixtures/measurement/v1/target-result.json")).unwrap();
    let golden: Proposal =
        serde_json::from_str(include_str!("fixtures/measurement/v1/proposal.json")).unwrap();
    let proposal = propose(
        &golden.basis_configuration,
        "1",
        &[PositionPair {
            anchor: a,
            target: b,
        }],
    )
    .unwrap();
    assert_eq!(proposal.changes, golden.changes);
    assert_eq!(proposal.status, "proposed");
    assert_eq!(proposal.basis_capture, golden.basis_capture);
    assert_eq!(
        proposal
            .candidate_configuration(&golden.basis_configuration, "1")
            .unwrap(),
        golden
            .candidate_configuration(&golden.basis_configuration, "1")
            .unwrap()
    );
    let refusal: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/measurement/v1/refusal.json")).unwrap();
    assert_eq!(refusal["contract"], "C-PA-MEASUREMENT-REFUSAL");
}
#[test]
fn owner_candidate_operation_rejects_forged_refused_or_mismatched_basis() {
    let proposal: Proposal =
        serde_json::from_str(include_str!("fixtures/measurement/v1/proposal.json")).unwrap();
    let request = serde_json::json!({"contract":"C-PA-MEASUREMENT","version":1,"operation":"candidate","configuration_revision":"1","configuration":proposal.basis_configuration,"proposal":proposal});
    let run = |value: &serde_json::Value| {
        execute(decode_request(&serde_json::to_vec(value).unwrap()).unwrap())
    };
    let candidate = run(&request).unwrap();
    assert_eq!(candidate["contract"], "C-PA-ALIGNMENT-CANDIDATE");
    let graph: GraphConfig =
        serde_json::from_str(candidate["configuration_json"].as_str().unwrap()).unwrap();
    assert_eq!(
        graph,
        proposal
            .candidate_configuration(&proposal.basis_configuration, "1")
            .unwrap()
    );
    let mut bad = request.clone();
    bad["proposal"]["changes"][0]["after_delay_samples"] = 481.into();
    assert!(run(&bad).is_err());
    let mut bad = request.clone();
    bad["proposal"]["status"] = "refused".into();
    assert!(run(&bad).is_err());
    let mut bad = request.clone();
    bad["configuration_revision"] = "2".into();
    assert!(run(&bad).is_err());
    let mut bad = request;
    bad["configuration"]["outputs"][0]["processing"]["limiter_db"] = (-20).into();
    assert!(run(&bad).is_err());
}

#[test]
fn crossover_overlap_uses_phase_model_and_independent_corrected_render() {
    let x = noise();
    let mut c = GraphConfig::stereo(vec![1000.]);
    for o in &mut c.outputs {
        o.source = Some(shr_pa::graph::Source::Input(0));
    }
    // High output 0 and low output 2; low driver adds 17 samples and inversion.
    let before = render(c.clone(), &x);
    let options = Options {
        max_arrival_samples: 128,
        band_hz: [500., 2000.],
    };
    let a = analyze(&meta("overlap-a", 0), &x, &before[0], &options).unwrap();
    let b = analyze(
        &meta("overlap-b", 2),
        &x,
        &shifted(&before[2], 17, -0.8),
        &options,
    )
    .unwrap();
    let pair = PositionPair {
        anchor: a,
        target: b,
    };
    let p = propose_overlap(&c, "1", std::slice::from_ref(&pair)).unwrap();
    assert_eq!(p.status, "proposed", "{}", p.reason);
    assert_eq!(p.added_latency_samples, 17);
    let reflected: Vec<_> = shifted(&before[2], 17, -0.8)
        .iter()
        .zip(shifted(&before[2], 83, -0.7))
        .map(|(a, b)| a + b)
        .collect();
    let reflected_pair = PositionPair {
        anchor: pair.anchor.clone(),
        target: analyze(&meta("reflected-low", 2), &x, &reflected, &options).unwrap(),
    };
    assert_eq!(
        propose_overlap(&c, "1", &[reflected_pair]).unwrap().status,
        "refused"
    );
    let prediction = predicted_sum(&pair, &p).unwrap();
    let after = render(p.candidate_configuration(&c, "1").unwrap(), &x);
    let low = shifted(&after[2], 17, -0.8);
    let sum: Vec<_> = after[0].iter().zip(low).map(|(a, b)| a + b).collect();
    let measured = analyze(&meta("sum", 0), &x, &sum, &options).unwrap();
    for b in measured.spectrum {
        let predicted = prediction
            .iter()
            .find(|p| p.frequency_hz == b.frequency_hz)
            .unwrap();
        assert!((20. * (b.magnitude / predicted.magnitude).log10()).abs() < 0.15);
    }
    let aligned = PositionPair {
        anchor: analyze(&meta("aligned-a", 0), &x, &after[0], &options).unwrap(),
        target: analyze(
            &meta("aligned-b", 2),
            &x,
            &shifted(&after[2], 17, -0.8),
            &options,
        )
        .unwrap(),
    };
    assert_eq!(
        propose_overlap(
            &p.candidate_configuration(&c, "1").unwrap(),
            "1",
            &[aligned]
        )
        .unwrap()
        .status,
        "no_change"
    );
    c.outputs[2].processing.eq[0] = shr_pa::config::EqBand {
        hz: 1000.,
        db: 12.,
        q: 2.,
        ..Default::default()
    };
    let distorted = render(c.clone(), &x);
    let pair = PositionPair {
        anchor: analyze(&meta("bad-a", 0), &x, &distorted[0], &options).unwrap(),
        target: analyze(&meta("bad-b", 2), &x, &distorted[2], &options).unwrap(),
    };
    assert_eq!(propose_overlap(&c, "1", &[pair]).unwrap().status, "refused");
}
#[test]
fn eq_fitter_independent_production_render_and_refusals() {
    use shr_pa::auto_eq::{Target, fit};
    let x = noise();
    let base = config();
    let mut room = config();
    room.outputs[0].processing.eq[0] = shr_pa::config::EqBand {
        hz: 2200.,
        db: 6.,
        q: 1.,
        ..Default::default()
    };
    let colored = render(room.clone(), &x);
    let measured = measure("eq", 0, &x, &colored[0]);
    let p = fit(&base, "1", std::slice::from_ref(&measured), Target::Flat).unwrap();
    assert_eq!(p.status, "proposed", "{}", p.reason);
    let candidate = p.candidate_configuration(&base, "1").unwrap();
    assert_eq!(candidate.inputs, base.inputs);
    assert_eq!(candidate.outputs[1], base.outputs[1]);
    let corrected = render(candidate, &colored[0]);
    let after = measure("eq-after", 0, &x, &corrected[0]);
    let error = |m: &Measurement| {
        (m.spectrum
            .iter()
            .map(|b| (20. * b.magnitude.log10()).powi(2))
            .sum::<f64>()
            / m.spectrum.len() as f64)
            .sqrt()
    };
    assert!(
        error(&after) < error(&measured) * 0.5,
        "{} -> {}",
        error(&measured),
        error(&after)
    );
    assert!((error(&after) - p.predicted_rms_db).abs() < 0.1);
    assert_eq!(
        fit(&base, "1", &[measure("flat", 0, &x, &x)], Target::Flat)
            .unwrap()
            .status,
        "no_change"
    );
    let mut bad = measured.clone();
    for b in &mut bad.spectrum {
        b.coherence = 0.2;
    }
    assert_eq!(
        fit(&base, "1", &[bad], Target::Flat).unwrap().status,
        "refused"
    );
    let mut conflict = measured.clone();
    conflict.capture.id = "eq2".into();
    conflict.capture.position_id = "p2".into();
    for b in &mut conflict.spectrum {
        b.magnitude *= 2.;
    }
    assert_eq!(
        fit(&base, "1", &[measured, conflict], Target::Flat)
            .unwrap()
            .reason,
        "eq_conflicting_positions"
    );
}
#[test]
fn sweep_known_impulse_calibration_noise_truncation_and_timing() {
    use shr_pa::sweep::*;
    let spec = SweepSpec {
        start_hz: 40.,
        end_hz: 16000.,
        sweep_samples: 16384,
        total_samples: 32768,
        amplitude: 0.1,
    };
    let x = spec.excitation().unwrap();
    let y = shifted(&x, 23, 0.5);
    let result = extract(&meta("sweep", 0), &spec, &x, &y, None).unwrap();
    assert_eq!(result.transfer.arrival_samples, 23);
    for b in &result.transfer.spectrum {
        assert!((b.magnitude - 0.5).abs() < 1e-8);
        let phase = b.phase_radians + 2. * PI * b.frequency_hz * 23. / 48000.;
        assert!(phase.sin().abs() < 1e-8);
    }
    assert!(!result.transfer.proposal_eligible);
    let mut clipped = y.clone();
    clipped[50] = 1.;
    assert!(extract(&meta("clip", 0), &spec, &x, &clipped, None).is_err());
    assert!(extract(&meta("short", 0), &spec, &x, &y[..y.len() - 1], None).is_err());
    assert!(extract(&meta("timing", 0), &spec, &shifted(&x, 1, 1.), &y, None).is_err());
    let noise = noise();
    let noisy: Vec<_> = y.iter().zip(noise).map(|(a, b)| a + b).collect();
    assert!(extract(&meta("noise", 0), &spec, &x, &noisy, None).is_err());
}

#[test]
fn overlap_conflicts_reflections_and_insufficient_excitation_refuse() {
    let x = noise();
    let c = config();
    let options = Options {
        max_arrival_samples: 128,
        band_hz: [500., 4000.],
    };
    let make = |id: &str, delay, position: &str| {
        let mut a = meta(&format!("{id}a"), 0);
        a.position_id = position.into();
        let mut b = meta(&format!("{id}b"), 1);
        b.position_id = position.into();
        PositionPair {
            anchor: analyze(&a, &x, &x, &options).unwrap(),
            target: analyze(&b, &x, &shifted(&x, delay, 1.), &options).unwrap(),
        }
    };
    assert_eq!(
        propose_overlap(&c, "1", &[make("a", 17, "p1"), make("b", 18, "p2")])
            .unwrap()
            .reason,
        "conflicting_positions"
    );
    let mut reflected = make("r", 17, "p1");
    let y: Vec<_> = shifted(&x, 17, 1.)
        .iter()
        .zip(shifted(&x, 83, 0.8))
        .map(|(a, b)| a + b)
        .collect();
    reflected.target = analyze(&meta("r-b", 1), &x, &y, &options).unwrap();
    assert_eq!(
        propose_overlap(&c, "1", &[reflected]).unwrap().status,
        "refused"
    );
    let mut weak = make("w", 17, "p1");
    for b in &mut weak.target.spectrum {
        b.coherence = 0.2;
    }
    assert_eq!(
        propose_overlap(&c, "1", &[weak]).unwrap().reason,
        "insufficient_coherent_overlap"
    );
}
#[test]
fn repeated_sweep_retains_origin_and_feeds_eq_with_explicit_calibration() {
    use shr_pa::{
        auto_eq::{Target, fit},
        calibration::Calibration,
        sweep::*,
    };
    let spec = SweepSpec {
        start_hz: 40.,
        end_hz: 16000.,
        sweep_samples: 16384,
        total_samples: 32768,
        amplitude: 0.03,
    };
    let x = spec.excitation().unwrap();
    let c = Calibration::parse(
        "SHR-PA-CAL 1 Hz correction_dB\n20 6.020599913279624\n20000 6.020599913279624",
        "invented-response-correction",
    )
    .unwrap();
    let y = shifted(&x, 23, 0.5);
    let r = extract_repeated(&meta("repeat", 0), &spec, &x, &vec![y.clone(); 4], Some(&c)).unwrap();
    let eq = shr_pa::auto_eq::fit_sweeps(&config(), "1", std::slice::from_ref(&r), Target::Flat)
        .unwrap();
    assert_eq!(eq.status, "no_change");
    assert!(eq.calibration.is_some());
    assert_eq!(r.transfer.arrival_samples, 23);
    assert!(r.reasons.is_empty());
    assert_eq!(
        r.calibration.unwrap().provenance,
        "invented-response-correction"
    );
    assert!(
        r.transfer
            .spectrum
            .iter()
            .all(|b| (b.magnitude - 1.).abs() < 1e-8 && b.coherence > 0.999)
    );
    assert_eq!(
        fit(&config(), "1", &[r.transfer], Target::Flat)
            .unwrap()
            .status,
        "no_change"
    );
    let mut wrong = vec![y; 4];
    wrong[3] = shifted(&x, 24, 0.5);
    assert!(extract_repeated(&meta("wrong", 0), &spec, &x, &wrong, None).is_err());
}
#[test]
fn eq_no_free_slots_nulls_and_target_definitions() {
    use shr_pa::auto_eq::{Target, fit};
    let x = noise();
    let mut m = measure("bounds", 0, &x, &x);
    let mut c = config();
    assert_eq!(Target::BassShaped.db(100.), 3.);
    assert_eq!(Target::BassShaped.db(200.), 0.);
    assert_eq!(Target::DampedHighFrequency.db(8000.), -3.);
    for b in &mut m.spectrum {
        b.magnitude = 2.;
    }
    for b in &mut c.outputs[0].processing.eq {
        b.db = 1.;
    }
    assert_eq!(
        fit(&c, "1", &[m.clone()], Target::Flat).unwrap().reason,
        "eq_no_unused_slots"
    );
    for b in &mut m.spectrum {
        b.magnitude = 0.01;
    }
    assert_eq!(
        fit(&config(), "1", &[m], Target::Flat).unwrap().reason,
        "eq_insufficient_reliable_band"
    );
}
#[test]
fn standalone_distance_entry_preserves_limits_and_other_parameters() {
    let mut editor = shr_pa::ui::Editor::new().unwrap();
    let original = editor.config;
    editor.command("delay pair 343cm");
    assert!((editor.config.bands[0].delay_ms - 10.).abs() < 1e-12);
    editor.command("delay pair 3.44m");
    assert!((editor.config.bands[0].delay_ms - 10.).abs() < 1e-12);
    editor.command("delay input 34.3m");
    assert!((editor.config.input_delay_ms - 100.).abs() < 1e-10);
    editor.command("delay pair 0ms");
    editor.command("delay input 0cm");
    assert_eq!(editor.config, original);
}

#[test]
fn sweep_two_tap_transfer_matches_independent_analytic_response() {
    use shr_pa::sweep::*;
    let spec = SweepSpec {
        start_hz: 40.,
        end_hz: 16000.,
        sweep_samples: 16384,
        total_samples: 32768,
        amplitude: 0.1,
    };
    let x = spec.excitation().unwrap();
    let y: Vec<_> = shifted(&x, 23, 0.5)
        .iter()
        .zip(shifted(&x, 61, 0.1))
        .map(|(a, b)| a + b)
        .collect();
    let r = extract(&meta("fir", 0), &spec, &x, &y, None).unwrap();
    for b in r.transfer.spectrum {
        let w = 2. * PI * b.frequency_hz / 48000.;
        let re = 0.5 * (w * 23.).cos() + 0.1 * (w * 61.).cos();
        let im = -0.5 * (w * 23.).sin() - 0.1 * (w * 61.).sin();
        assert!((b.magnitude - re.hypot(im)).abs() < 1e-8);
    }
}

#[test]
fn eq_narrow_null_and_unimprovable_alternation_preserve_base() {
    use shr_pa::auto_eq::{Target, fit};
    let x = noise();
    let base = config();
    let mut m = measure("null", 0, &x, &x);
    m.spectrum[20].magnitude = 0.01;
    let p = fit(&base, "1", &[m.clone()], Target::Flat).unwrap();
    assert_eq!(p.status, "no_change");
    assert!(p.bands.is_empty());
    assert_eq!(p.basis_configuration, base);
    for (i, b) in m.spectrum.iter_mut().enumerate() {
        b.magnitude = 10_f64.powf(if i % 2 == 0 { 0.8 / 20. } else { -0.8 / 20. });
    }
    let p = fit(&base, "1", &[m], Target::Flat).unwrap();
    assert_eq!(p.status, "refused");
    assert!(p.bands.is_empty());
    assert_eq!(p.reason, "eq_no_safe_improvement");
}

#[test]
fn explicit_bass_and_damped_targets_are_not_diluted_by_dense_high_frequency_bins() {
    use shr_pa::auto_eq::{Target, fit};
    let x = noise();
    let measured = measure("targets", 0, &x, &x);
    for target in [Target::BassShaped, Target::DampedHighFrequency] {
        let p = fit(&config(), "1", std::slice::from_ref(&measured), target).unwrap();
        assert_eq!(
            p.status, "proposed",
            "{target:?}: {} / {} dB",
            p.reason, p.before_rms_db
        );
        let after = render(p.candidate_configuration(&config(), "1").unwrap(), &x);
        let actual = measure("target-after", 0, &x, &after[0]);
        // Independent log-frequency quadrature of the actual rendered transfer.
        let error = |m: &Measurement| {
            let mut sum = 0.;
            let mut weight = 0.;
            for (i, b) in m.spectrum.iter().enumerate() {
                let lo = if i == 0 {
                    b.frequency_hz
                } else {
                    (m.spectrum[i - 1].frequency_hz * b.frequency_hz).sqrt()
                };
                let hi = if i + 1 == m.spectrum.len() {
                    b.frequency_hz
                } else {
                    (m.spectrum[i + 1].frequency_hz * b.frequency_hz).sqrt()
                };
                let w = (hi / lo).ln();
                sum += w * (20. * b.magnitude.log10() - target.db(b.frequency_hz)).powi(2);
                weight += w;
            }
            (sum / weight).sqrt()
        };
        assert!(error(&actual) + 0.2 < error(&measured));
        assert!((error(&actual) - p.predicted_rms_db).abs() < 0.1);
    }
}

#[test]
fn eight_position_prediction_display_preserves_positions_and_endpoints_within_budget() {
    let x = noise();
    let a = measure("a", 0, &x, &x);
    let b = measure("b", 1, &x, &shifted(&x, 17, 1.));
    let mut positions = Vec::new();
    for i in 0..8 {
        let mut a = a.clone();
        let mut b = b.clone();
        a.capture.id = format!("a{i}");
        b.capture.id = format!("b{i}");
        a.capture.position_id = format!("p{i}");
        b.capture.position_id = format!("p{i}");
        positions.push(PositionPair {
            anchor: a,
            target: b,
        });
    }
    let result = execute(Request::Overlap {
        contract: "C-PA-MEASUREMENT".into(),
        version: 2,
        configuration_revision: "1".into(),
        configuration: config(),
        positions,
    })
    .unwrap();
    let predictions = result["predictions"].as_array().unwrap();
    assert_eq!(predictions.len(), 8);
    assert_eq!(
        predictions
            .iter()
            .map(|p| p.as_array().unwrap().len())
            .sum::<usize>(),
        128
    );
    for p in predictions {
        let bins = p.as_array().unwrap();
        assert_eq!(
            bins[0]["frequency_hz"].as_f64(),
            Some(a.spectrum[0].frequency_hz)
        );
        assert_eq!(
            bins.last().unwrap()["frequency_hz"].as_f64(),
            Some(a.spectrum.last().unwrap().frequency_hz)
        );
    }
    assert!(serde_json::to_vec(&result).unwrap().len() < 65536);
}
