# SHR PA: plan and implementation

SHR PA owns speaker processing/protection, measurement, alignment and prepared
embedding. GigPies owns program mixing, physical audio devices and final integration.

## Delivered contract and evidence

PA-01 fixed-v1 descriptor/status and PA-02 configurable C-PA v2 have software
acceptance. V2 supports dynamic program inputs, weighted DAG routing and independent
speaker outputs, EQ/dynamics/delay/polarity and sample protection; v1/standalone
compatibility remains. [Embedding v2](EMBEDDING_V2.md),
[owner verification](verification/0017-configurable-embedding.md) and the GigPies
modular acceptance record own exact evidence. Acoustics and physical protection
remain unqualified; sample limiting is not true-peak or speaker calibration proof.

## Next work

Measurement and delay/polarity alignment have one owning task plan:
[PHASE_ALIGNMENT](PHASE_ALIGNMENT.md). Update its plan/checklist/evidence in place;
do not copy its P4/P5 tasks here or into GigPies. The existing GP20 host adapter is a distinct integration; its current extensions
are recorded by shared [GP-PA-SETUP](https://github.com/gigpies/gigpies/blob/main/docs/archive/pa-setup-2026-10-29.md). Physical graph
qualification contributes to shared GP-H1. Other standalone PA features stay in
[the standalone roadmap](ROADMAP.md), outside GigPies's integration queue.

## GP-H1 detached EQ context contribution

The shared [GigPies GP-H1 card](https://github.com/gigpies/gigpies/blob/main/docs/development/MODULE_IMPLEMENTATION_PLAN.md#gp-h1--physical-audio-qualification)
owns source execution integration and acceptance. The additive
[detached context ABI](LIVE_EQ_ABI.md#detached-context-extension) supplies bounded
source-side observation and independent controller preparation/readback without
changing DSP, live-EQ transition semantics or the original embedding interfaces.
Owner tests and downstream actual-library checks do not establish full source
isolation or hardware acceptance.

Validation on 2026-11-12 (Pi5, Rust 1.97.1): the 18 focused EQ/ABI/allocation
tests and all 122 normal Rust tests passed, along with formatting, warning-denied
Clippy, five publication-helper tests and documentation checks. The locked release
build, terminal and software-null control checks passed. Release smoke checks
confirmed source-preserving v2 migration, six-channel 48 kHz float WAV output and
40×13 snapshots; temporary outputs were removed. Commands follow
[VALIDATION](VALIDATION.md) and [the EQ ABI](LIVE_EQ_ABI.md).
Historical research, load/thermal tests and physical hardware acceptance were
intentionally not run. This completes the module contribution; downstream
integration and GP-H1 acceptance remain separate.

## Tracking and verification

This is the owning plan for module-only GigPies work. Keep each new task's plan,
implementation state, checklist, evidence and next action together here. Shared
integration tasks live only in the [GigPies integration plan](https://github.com/gigpies/gigpies/blob/main/docs/development/MODULE_IMPLEMENTATION_PLAN.md);
link to their cards instead of copying status. Follow its task lifecycle and this
repository's AGENTS.md. STATUS/acceptance files hold dated evidence, not another queue.
Preserve closed milestone details in the linked archive; do not reopen old launch cards.
A documentation reconciliation does not rerun tests or qualify hardware.

[Historical plan and milestone evidence](archive/tracking-before-2026-10-09/GIGPIES_IMPLEMENTATION.md). Its launch instructions are retired.


[PA-CAL-REVIEW — calibrated offline Desk review](archive/pa-cal-review-2026-10-29.md) has software acceptance; physical qualification remains separate.
