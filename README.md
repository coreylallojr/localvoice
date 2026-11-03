# LocalVoice

LocalVoice is a fully local, low-latency voice changer for Windows, written in Rust. It captures your microphone, applies a voice, and sends the result to a virtual microphone, so Discord, games, OBS and browsers hear the changed voice. It uses about 1–3% of one CPU core, so you can leave it running while you game.

No account, cloud service, telemetry or network connection is used.

## Features (v0.3.0)

- **Natural pitch and formant shifting.** A phase-vocoder shifter with spectral-envelope formant control: deep or high voices without the chipmunk effect, or cartoon shifts when you want them.
- **Noise removal.** RNNoise via [nnnoiseless](https://github.com/jneem/nnnoiseless) removes keyboard, fan and room noise. Also included: a hysteresis noise gate, compressor, EQ, grit, bit crush, ring modulator, chorus, Freeverb reverb and a limiter.
- **Eight built-in voices:** Clean, Deep, Bright, Demon, Robot, Radio, Giant and Sprite.
- **Voice editor with live preview.** Every slider changes the sound as you drag it. Save, save as new, revert, and import/export `.voicepack` files (v0.2 packs still load).
- **Push-to-talk voice.** Your mic always transmits; the key switches the voice effect on. *Hold* uses the effect only while the key is held; *Toggle* switches it on/off with each press. Any key or mouse side button works, and switching is crossfaded so it never clicks.
- **Global hotkeys that work inside games:** Ctrl+Shift+Space go live, Ctrl+Shift+M mute, Ctrl+Shift+B effect on/off, Ctrl+Shift+Left/Right switch voice.
- **Headphone test and monitor**, mic gain, headphone volume, level meters, and latency/DSP/dropout readouts.
- **Smart device defaults.** Picks VB-CABLE's working `CABLE Input` endpoint (skips the broken `CABLE In 16ch`) and your headset for monitoring.
- **System tray**, single instance, and settings that persist in `%APPDATA%\LocalVoice`.

## Setup

1. Install [VB-CABLE](https://vb-audio.com/Cable/): extract the ZIP, run `VBCABLE_Setup_x64.exe` as administrator, and restart Windows.
2. Download `LocalVoice.exe` from the [latest release](https://github.com/coreylallojr/localvoice/releases/latest) and run it. Windows SmartScreen may warn because the build is not code-signed; choose **More info → Run anyway**.
3. Check the three devices: your microphone, `CABLE Input` as the virtual mic, and your headphones.
4. Click **TEST IN HEADPHONES** (wear headphones) and pick a voice.
5. Click **GO LIVE**. In Discord, a game or OBS, choose **CABLE Output** as the microphone.

Keep your Windows default playback device on your speakers or headset, not on CABLE Input. Otherwise all PC audio goes into the virtual mic.

## Build from source

Requirements: Windows 10/11, [Rust](https://rustup.rs) (stable, MSVC toolchain) and the Visual Studio C++ Build Tools.

```powershell
git clone https://github.com/coreylallojr/localvoice.git
cd localvoice
.\install.ps1     # builds release, installs to %LOCALAPPDATA%\Programs\LocalVoice, adds `localvoice`
localvoice        # start it from any new terminal
```

## Development

```powershell
cargo test --release                   # DSP, voices, devices, hotkeys, settings
cargo clippy --all-targets -- -D warnings
cargo run --release -- --soak 30       # real-device self-test: tone -> CABLE loopback pitch check + 30 s soak
```

The code is in `src/`:

| File | Contents |
| --- | --- |
| `dsp/` | Voice chain, pitch shifter and effects |
| `engine.rs` | Audio streams and drift control |
| `app.rs` | Controller, shared by the UI, hotkeys and tray |
| `ui.rs` | egui window |
| `devices.rs`, `voices.rs`, `hotkeys.rs` | Devices, voices and hotkeys |

## Credits

LocalVoice builds on:
- [cpal](https://github.com/RustAudio/cpal) for audio I/O
- [egui/eframe](https://github.com/emilk/egui) for the UI
- [nnnoiseless](https://github.com/jneem/nnnoiseless) (RNNoise) for noise removal
- [realfft](https://github.com/HEnquist/realfft) for FFTs
- [biquad](https://github.com/korken89/biquad-rs) for filters
- [tray-icon](https://github.com/tauri-apps/tray-icon) for the system tray

The pitch shifter follows Stephan Bernsee's smbPitchShift design, and the reverb is Jezar's public-domain Freeverb. The effect-chain design was informed by [Voclo](https://github.com/oonta/voclo).

LocalVoice is GPL-3.0-only.
