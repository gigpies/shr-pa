# Offline PA setup and review

The measurement owner operations use invented or operator-supplied offline data.
They open no microphone or playback stream, change no running configuration and
never rearm. Distance entry uses the existing editor transaction behavior.
They do not establish acoustic alignment, room correction, microphone accuracy,
physical clock lock or speaker protection. Existing measurement v1 requests and
fixed/configurable DSP ABIs are unchanged.

## Distance entry

In the standalone editor enter `:delay input 1m` or `:delay pair 34.3cm`.
`pair` means the selected output pair. `ms`, `m`, `cm`, and bare milliseconds
are accepted; negative, nonfinite, unknown-unit and out-of-range values refuse.
Only `delay_ms` is saved. Changing entry units does not change a second parameter.
Input delay remains 0–100 ms; output remains 0–10 ms. Existing transitions apply.
The gain/delay page displays the integer-sample applied value at the configuration
sample rate, milliseconds, and approximate distance. Distance is **added acoustic
path equivalent at 343 m/s**, not a measured speaker-to-listener distance. At
48 kHz one sample is 0.020833 ms or about 0.715 cm; no millimetre precision is claimed.

## Additive owner request version 2

Use the existing `shr-pa-measure REQUEST.json` (or `-` for stdin) with contract
`C-PA-MEASUREMENT`, `version: 2`. The existing byte, duplicate-key, nesting and
array limits still apply. Operations below are offline and bounded. Version 1
operations keep their original schema and conservative correlation thresholds.

- `overlap`: `configuration_revision`, `configuration`, `positions` use the v1
  proposal request shapes. Returns `C-PA-OVERLAP-REVIEW:1` containing `proposal`
  (the unchanged delay/polarity proposal shape) and `predictions` (one spectrum
  per position). The review has at most 128 predicted display bins total,
  uniformly selecting endpoints and intermediate bins per position when needed;
  analysis still uses every qualified bin. Only matched bins with coherence >=0.98 and relative magnitude
  between 0.1 and 10 contribute; require eight bins spanning at least 200 Hz.
  Search integer delay differences -480..480 and both polarities, minimizing
  wrapped relative phase RMS. Boundary solutions and RMS above 15 degrees refuse.
  All positions must agree on the exact sample/polarity correction. Predictions
  are complex sums of measured transfers after the proposed changes, not fresh
  measurements. Frequency-dependent phase that does not fit this model refuses.
  Correlation arrival includes crossover/filter behavior; it is not independently
  established acoustic travel time. Strong coherent reflections can still be ambiguous. No arbitrary phase filter,
  fractional delay or longer output delay is introduced.
- `eq_propose`: `configuration_revision`, `configuration`, `positions` (one to four
  measurement results of the same output, distinct IDs/positions, shared source,
  map, reference, offset, rate and configuration), and `target` below. Returns
  `C-PA-EQ-PROPOSAL:1`, explicitly separate from alignment proposals.
- `eq_sweeps`: same configuration, revision and target as `eq_propose`, but
  `positions` contains complete qualified C-PA-SWEEP-RESULT documents. Require
  matching calibration objects/provenance across all positions. Calibration is
  retained in the EQ proposal; uncorrected H1 proposals explicitly carry null.
- `eq_candidate`: current `configuration_revision`, `configuration`, and the EQ
  `proposal`. Returns `C-PA-EQ-CANDIDATE:1` with basis capture/revision and detached
  `configuration_json`. Only proposed results produce candidates. Exact base
  equality and bounds are checked; current host identity must also match.
- `sweep` and `sweep_repeated`: described below. Their result's `transfer` can feed
  the same owner EQ/overlap functions when qualified. Single sweeps cannot qualify
  a correction. Keep the containing result with calibration provenance.

## Conservative EQ objective

Target levels are relative to the measured reference, never SPL:

| Target | Numerical definition at frequency f in Hz |
|---|---|
| `flat` | 0 dB |
| `bass_shaped` | clamp(3 log2(200/f), 0, 3) dB |
| `damped_high_frequency` | clamp(-1.5 log2(f/2000), -6, 0) dB |

Aggregate equal-weight log magnitudes, not unaligned waveforms. Require coherence
>=0.98 at every retained position; >4 dB positional spread refuses the entire
proposal. The RMS objective weights log-frequency intervals (geometric midpoints
between adjacent retained frequencies), so dense high-frequency bins cannot dilute
the bass target. Exclude magnitude below -12 dB, unreliable bins, and frequencies outside
40–16000 Hz. Require twelve bins spanning two octaves. An RMS error <=0.75 dB
returns no change. Fit only unused (zero-gain) slots in an already-enabled output
PEQ bank; preserve existing nonzero bands. If no slots remain, refuse.

The finite greedy grid uses measured center frequencies within 50–12000 Hz,
Q in {0.5,1,2}, and gains {-6,-5,-4,-3,-2,-1,1,2,3} dB, at most eight slots.
Coefficients come from production PA EQ. Each step must improve RMS by >0.05 dB
without worsening any included bin's absolute error by >1 dB; total improvement
must be at least 0.25 dB. Sum of positive gains <=3 dB, sum of cuts >=-12 dB.
Required extra headroom is conservatively the sum of positive gains; it is not
silently supplied through gain or limiter changes. Routing, crossovers, protection,
other output banks, program processing and mute settings remain identical.

These are linear-transfer predictions; changing dynamics activity can invalidate
that model and requires new measurement. This is a bounded candidate search, not
a globally optimal room inversion. Sparse
bins can miss narrow features; deep cancellation nulls are excluded rather than
filled; any deep null disables boost throughout that fit. Inspect the complete candidate and headroom, apply through muted review,
then obtain new independent measurements. A refused or unchanged result applies
nothing. Physical suitability remains unqualified.

## Supported microphone correction format

Only this explicitly defined text format is supported by `Calibration::parse`:

```text
SHR-PA-CAL 1 Hz correction_dB
20 -1.0
1000 0.0
20000 1.5
```

The header is exact. Following nonempty rows contain exactly two whitespace-separated
finite numbers: frequency in Hz and correction in dB. Positive correction increases
the reported transfer magnitude; a microphone reading +2 dB requires -2 dB.
Require 2–256 strictly ascending unique frequencies in 10–24000 Hz, corrections
within ±20 dB, text <=32768 bytes, and a separately supplied 1–128 byte provenance
identifier. Unknown headers, comments, extra columns, duplicate/unsorted frequencies,
nonfinite numbers and out-of-range data refuse. Correction interpolates linearly
in dB against log frequency; there is no extrapolation. Every analyzed frequency
must be covered. Phase, delay and absolute SPL are not calibrated. This is not
universal vendor-file support. Preserve the original calibration and provenance.

Convert an existing regular text file without opening audio:
`shr-pa-measure --calibration /absolute/path/mic.txt PROVENANCE_ID`.
The JSON representation is `{ "provenance": "operator-id", "points": [[20,-1],
[1000,0],[20000,1.5]] }`. The same validation applies. No file is downloaded.

## Bounded offline sweep extraction

Supply `capture` (existing common-origin metadata), `spec`, `reference`, `mic`, and
`calibration` (object above or null). `spec` contains `start_hz`, `end_hz`,
`sweep_samples`, `total_samples`, `amplitude`. `SweepSpec::excitation()` generates
only an in-memory logarithmic swept sine at 48 kHz: phase is
2π f0 T / ln(f1/f0) × (exp(t ln(f1/f0)/T)−1), with linear 256-sample end fades.
No automatic playback exists. Reference samples must match that known excitation
within 1e-10; mismatched start/timing, nonfinite/clipped samples, unequal/truncated
buffers and invalid capture timing/drop flags refuse.

Limits: 20–20000 Hz, at least two octaves, 8192–32768 sweep samples, total length
32768 or 65536 with >=4096 silent tail samples, amplitude 0.001–0.25. Response
mean square must exceed 1e-10; the final 2048-sample mean square must be <=1e-4
of the complete response mean square. This tail gate refuses noisy or insufficiently
decayed records; it does not prove linearity or correct hardware timing.

A bounded radix-two FFT divides response by known excitation only where reference
power exceeds 1e-6 of peak. Retain up to 64 log-spaced bins over 1.25 f0..f1/1.25,
with reference power >=1e-4 of peak. Report the first 4096 deconvolved impulse
samples. An absolute impulse maximum outside 0..2047 refuses. Never independently
zero or shift speaker-path onsets; original reference offset/timeline stays intact.
A single sweep reports coherence unavailable (zero sentinel, reason, ineligible).

`sweep_repeated` replaces `mic` with exactly four `responses`, sharing the same
known reference and common-origin timing. Individual quality gates still apply;
arrival disagreement refuses. Complex-average transfer and ensemble coherence
|ΣH|²/(4Σ|H|²) are computed without arrival shifting. Twelve bins and coherence
>=0.98 are required for downstream fitting. The algorithm is explicitly
`offline-sweep-ensemble-v1`, not the windowed H1 algorithm. Repeat coherence does
not detect a repeatable nonlinearity, bad calibration, or common timing error.

`C-PA-SWEEP-RESULT:1` retains `transfer`, `calibration`, `impulse`, and `reasons`.
The `transfer` is accepted by the new offline owner operations; it must not be
passed off as a GP20 v1 captured H1 record. GP20 continues to own live-source
software capture; offline sweep files do not introduce another capture owner.

[Software validation record](archive/pa-setup-2026-10-29.md).
