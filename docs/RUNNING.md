# Use the PA processor

[Project home](../README.md) · [All controls and command reference](reference/RUNNING.md) · [Engineering](ENGINEERING.md)

This guide covers the standalone two-input/six-output editor. In GigPies, use
[Desk's PA controls](https://github.com/gigpies/shr-desk/blob/main/docs/USING.md#master-eq).

## Start offline

Linux requires a C linker, the pinned Rust toolchain, `pkg-config` and ALSA
headers (`libasound2-dev` on Debian/Ubuntu). From the checkout:

```sh
cargo build --release --locked
SHR_PA_BIN="$PWD/target/release/shr-pa"
mkdir -p artifacts
SESSION=$(mktemp -d "$PWD/artifacts/session-XXXXXX")
"$SHR_PA_BIN" init "$SESSION/preset.json"
(cd "$SESSION" && "$SHR_PA_BIN")
```

Use a fresh session: `init` replaces an existing destination. The editor opens
no audio device. Press **l** to import `preset.json`; it is not loaded automatically.
The starting three-way preset is generic and contains no speaker tuning.

## Edit and preview

Use a terminal of at least **40×13** cells. Arrows/Tab or the footer change pages.

| Action | Key |
|---|---|
| Select a processing module | v |
| Select a parameter / adjust it | n/N / x/X |
| Select an EQ band / input side / output pair | j/k / h / b |
| Mute / unmute all | m / u |
| Toggle one output mute | 1–6 |
| Calculate an offline meter preview | r |
| Save / load `preset.json` | s / l |
| Preset library / exit | P / q |

Try **u**, then **r** for a silent computed preview. Edit a value and press r
again. Crossover mode **Layout LR24** uses the layout's filters; **Independent**
exposes separate HP/LP edges. Switching back discards custom edge settings, so
save them first. [Full crossover controls](reference/RUNNING.md#crossover-controls).

## Save and recall

**s** exports JSON; press twice to confirm replacement. In the library, type
`:select U1`, then `:save My venue` to save to an empty slot. Use `:recall` for
the selected slot. Recall mutes all outputs; wait for pending/busy to clear
before unmuting again. Working edits recover muted on restart.

Back up `preset.json` and the session's `.shr-pa` directory with editors stopped.
[Library commands and migration](reference/RUNNING.md#local-library-and-working-recovery).

## Connect audio deliberately

```sh
"$SHR_PA_BIN" devices
"$SHR_PA_BIN" live "$SESSION/preset.json"   hw:CARD=CARD_ID,DEV=0 hw:CARD=CARD_ID,DEV=0   2 2 0,1 0,1,-,-,-,- 10 --ui
```

Replace CARD_ID with your device. Capture and playback must share one card/clock.
The six assignments are **H-L, H-R, M-L, M-R, L-L, L-R**; indices start at zero
and `-` leaves an output disconnected. This example routes only the high pair.
It does not mix the six outputs down to stereo. Live starts muted; verify the
patch before pressing u. [Complete device setup](reference/RUNNING.md#direct-alsa-explicit-hardware-and-channel-selection).

## Problems

| Display | What to do |
|---|---|
| Pending/busy | Wait for the change to settle before unmuting. |
| NO STORAGE | Close the other editor or resolve the directory error, then restart. |
| Unsaved | Resolve storage failure; recent edits may not survive restart. |
| Recovery BLOCKED | Preserve the files; follow the recovery procedure. |
| Stream fault | Investigate, then restart explicitly. Sound does not resume automatically. |

[Recovery and backup details](reference/RUNNING.md#backup-recovery-and-rollback).
Software mute cannot guarantee silence after a device, process or power failure.
