# What works today

[Use the processor](RUNNING.md) · [GigPies integration](GIGPIES_IMPLEMENTATION.md) · [Full DriveRack function map](DRIVERACK_MAP.md)

**The PA DSP, test generators and offline measurement tools are implemented.**
Configurable PA processing and software measurement review are integrated into
GigPies. A complete microphone-driven acoustic setup session is still unfinished.

## Processing and operation

| Available now | Where and limits |
|---|---|
| Crossovers | Standalone full-range, two-way and three-way layouts; independent Butterworth 6–48 and Linkwitz–Riley 12/24/36/48 edges. Configurable embedding also supports multiway graphs, including compensated LR24 three/four-way references. |
| Routing and output controls | Standalone two inputs/six logical outputs with paired processing; GigPies uses dynamic program inputs, explicit weighted routing and independent speaker outputs. Physical capacity depends on the configured interface. |
| EQ | 31-band graphic EQ and eight-band bell/shelf parametric EQ; input and speaker EQ banks. Standalone flatten/restore and original tonal curves; prepared live input-EQ extension for embedding. |
| Dynamics and protection | Compression, sample-peak limiting, reduction/peak/clip meters, ramped mutes and numerical-fault silence. These are implemented DSP protections; amplifier/speaker calibration and acoustic safety are not established. |
| Delay and polarity | Input delay 0–100 ms and output delay 0–10 ms; independent outputs in configurable embedding, linked band pairs in standalone. Integer-sample resolution, polarity and transitioned edits. Standalone accepts ms/m/cm distance-equivalent entry. |
| Test generators | Seeded pink/white noise, sine, impulse and logarithmic sweep. Offline rendering and explicitly selected standalone live source; adjustable level and ramped off/on restoring mapped capture. Source type is selected when starting the session. |
| Standalone operation | Offline terminal editor, six-channel WAV renders, explicit ALSA capture/playback mappings, prepared live edits and fault-stop behavior. Live starts muted. |
| Presets and recovery | 75 named user slots, six immutable layout templates, explicit save/recall/copy, JSON migration and working-edit recovery. Generic layouts are not manufacturer speaker tunings. |

Use the [operating guide](RUNNING.md) for controls and the [DSP contract](DSP.md)
and [configurable embedding contract](EMBEDDING_V2.md) for exact behavior.

## Measurement and setup

| Available now | What remains |
|---|---|
| Reference/microphone analysis | Offline 48 kHz magnitude, phase, coherence and integer arrival analysis exist. Synchronized physical microphone capture remains unavailable in the integrated measurement provider. |
| Delay/polarity alignment proposals | The owner compares speaker paths, fits crossover-overlap phase and predicts their sum. It can return a proposed change, no change or refusal; it does not automatically apply settings. Real combined-response verification is pending. |
| Sweep analysis | Known logarithmic-sweep extraction, impulse evidence, four-repeat coherence and a defined microphone magnitude-correction format exist. Supplied offline recordings do not establish hardware timing or calibration accuracy. |
| EQ proposals | Conservative output-PEQ fitting, explicit target curves and multi-position review exist. This is an offline proposal tool, not a complete live AutoEQ wizard. |
| GigPies/Desk review | Software capture/review and imported calibrated repeated-sweep EQ review use the existing muted prepare/review/commit path. Rearm is explicit; no analysis request starts excitation or opens a microphone. |

See [offline setup](OFFLINE_SETUP.md) for supported operations and limits,
[measurement v1](MEASUREMENT_V1.md) for the original analyzer, and
[the alignment plan](PHASE_ALIGNMENT.md) for the remaining work. Pink generation,
sweep generation and sweep analysis are separate capabilities: their presence does
not imply an automated physical measurement session.

## Still to implement or qualify

- Physical synchronized reference/mic acquisition, the measurement RTA display,
  automated excitation/capture sequencing and measured before/after verification.
- Complete setup/level-balancing/AutoEQ wizards and verified speaker/amplifier profiles.
- Feedback suppression, guided ring-out and subharmonic synthesis.
- Remaining PA2 compatibility details, including shelf-slope units, limiter
  bypass/knee modes and automatic input-EQ restore sources.
- Remaining standalone utility/lockout/remote/update flows, automatic device
  recovery and physical display/control acceptance.
- Full multichannel interface/ADAT mapping, sustained operation, acoustic alignment
  and physical speaker protection. Software tests do not close these gates.

The [DriveRack map](DRIVERACK_MAP.md) retains the complete PA2 target inventory;
[the roadmap](ROADMAP.md) groups the unfinished work. Implemented portions of a
partially complete feature remain available.

## Evidence

Normal regression coverage includes DSP, routing, configuration, measurement,
proposal/refusal behavior, allocation bounds, recovery, terminal controls and
explicit software-null streaming. [Validation](VALIDATION.md) lists reproducible
commands; [verification records](verification/README.md) retain dated results.
The [offline setup evidence](archive/pa-setup-2026-10-29.md) and
[calibrated review evidence](archive/pa-cal-review-2026-10-29.md) record the delivered
software setup increments.

A limited stereo AudioBox bench exercised PA processing. Its unresolved right
route and small timing discontinuities do not establish full acoustic or
multichannel acceptance. [Historical PA records](archive/status-before-2026-11-11.md)
preserve those observations and earlier test results without treating them as
current hardware guarantees.
