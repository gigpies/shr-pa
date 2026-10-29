# PA-CAL-REVIEW — calibrated offline Desk review

State: **ACCEPTED (software only)**, 2026-10-29. Single-threaded canonical-checkout work.

Owner validation of imported calibrated sweep evidence; retain calibration, common origins and exact configuration. Repair standalone CI links.

Plan: strengthen imported sweep validation in the PA owner; provide a reproducible invented fixture; preserve v1/v2 measurement and DSP compatibility. Provider authority and actual-owner application are tracked only in the shared integration card. No automatic apply/rearm. No devices (including null PCM), services, hardware or load tests.

Acceptance:
- [x] Bounded evidence and calibration provenance, common origin and configuration identity preserved.
- [x] Standalone owner operations remain offline, detached and device-free.
- [x] Missing impulse, calibration coverage and incompatible-origin refusals pass.
- [x] 114 normal tests pass; four PCM tests filtered. Clippy/release and real C v1/v2 callers pass.
- [x] Separate physical procedure documented; disposable outputs cleaned.

Evidence: baseline PA f6be5ae, Desk 511a811, GigPies bddb503; all three checkouts clean. PA published CI fails sibling-relative documentation links (fixed locally); Desk and GigPies published CI pass.

Next action: separately authorized physical qualification; commit/push only on user request. No pending software blocker in this scope.

Shared integration owner: [GP-CAL-REVIEW](https://github.com/gigpies/gigpies/blob/main/docs/archive/pa-cal-review-2026-10-29.md).

## Owner validation

`fit_sweeps` additionally refuses missing/nonfinite 4096-sample impulse evidence
and calibration without full transfer-band coverage. Existing common-origin,
configuration, quality and calibration-consistency checks remain. The repeatable
`offline-review-fixture GRAPH_JSON REVISION` example produces invented calibrated
sweep input for provider/Desk compatibility; it opens no devices.

Rust 1.97.1, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=1, canonical target:
114 normal tests passed with the four PCM-opening integrations explicitly filtered
as documented in [validation](../VALIDATION.md#strict-no-device-pa-setup-validation).
All-target Clippy with `-D warnings`, release build, five Python policy tests,
publication guard, formatting, frozen measurement corpus and real C v1/v2 callers
(all three v2 graph fixtures) passed. The documentation checker also passed with
all paths outside the repository treated as absent, reproducing CI isolation.

Published CI f6be5ae failed only the sibling-relative links in the implementation
plan and prior acceptance record; both now use repository URLs. The correction
is local and uncommitted. Physical procedure: [phase alignment](../PHASE_ALIGNMENT.md#physical-qualification-after-imported-sweep-review).
No null PCM, physical, playback, service or load checks were run.

## Authorized publication follow-up

The user subsequently authorized commit/push and remote CI verification. Run
[40919704329](https://github.com/gigpies/shr-pa/actions/runs/40919704329) at
2c8ddaf passed documentation, Rust tests and release builds on both architectures,
then exposed an older terminal assertion expecting `EQ +1dB`. The current compact
screen displays `EQ +1`. The assertion now matches that exact value through the
next ANSI boundary; persisted EQ-value and recovery assertions remain unchanged.
`python3 scripts/check-terminal.py target/release/shr-pa` passes all eight offline
terminal/recovery checks locally. Its temporary working state was automatically
removed. No PCM opens, playback or host-setting changes were involved. This is a
test-only correction; the validated production source identity is unchanged.

The follow-up CI run at 6ad1e6e passed the corrected terminal check and exposed
a second stale label in the existing live-controls check: `Input delay 1.0 ms`
versus the current `Input 1.000ms ~0.34m/34cm`. Its regex now checks the exact
current delay/distance text; the saved numeric delay assertion remains unchanged.
Python syntax and positive/negative matching against CI text pass locally.
The null-PCM script was not invoked locally under the session's no-device limit;
its execution is left to the existing GitHub CI workflow. No production change.
