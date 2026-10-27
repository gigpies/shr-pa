![SHR PA](docs/assets/banner.svg)

# SHR PA

**PA processing for GigPies, with a standalone six-output editor.** Crossovers,
parametric and graphic EQ, compression, limiters, delay, polarity and output mutes.

![Standalone offline editor](docs/assets/terminal.svg)

[**Use the processor**](docs/RUNNING.md) · [**GigPies guide**](https://github.com/gigpies/gigpies/blob/main/docs/README.md) · [Engineering](docs/ENGINEERING.md)

## Choose your setup

- **In GigPies:** configurable PA inputs and outputs, controlled from Desk.
- **Standalone:** two inputs and six logical outputs, with full-range, two-way
  and three-way layouts in a compact 40×13 terminal.
- **Offline:** edit presets and render six-channel WAV previews without hardware.

The standalone editor starts offline. Live operation uses explicitly selected
ALSA devices and starts muted. Presets are generic starting points, not speaker tunings.

**Experimental, 0.2 alpha.** DSP and software integration work; physical speaker
protection and acoustic alignment remain unqualified. Offline measurement can
propose delay/polarity changes; complete automatic acoustic setup is unfinished.

## First session

[Build and open a fresh session](docs/RUNNING.md#start-offline), then load or edit
a preset. The guide covers controls, saving, output assignments and recovery.
There is no need to read the DSP or integration specification first.

[Current limits](docs/STATUS.md) · [Release notes](CHANGELOG.md) · [MIT license](LICENSE)
