# LocalVoice

LocalVoice is a fully local, real-time Windows voice changer. It captures a physical microphone, applies a character preset, and sends the result to any output device. Point it at a virtual audio cable and every application can use the changed voice as an ordinary microphone.

No account, cloud service, telemetry, or network connection is used while running.

## What works in v0.1

- Live microphone processing with eight bundled characters
- Pitch shift, gate, EQ, compression, distortion, bit crushing, chorus and reverb
- Global hotkeys for live/stop, mute, bypass and character switching
- Full custom character editor for pitch, gate, tone, grit, bit depth, chorus, reverb and gain
- Safe `.voicepack` import and export
- Input/output meters and fail-silent audio errors
- Any Windows input/output device supported by PortAudio/WASAPI

## Run it

Requirements: Windows 10/11 and Python 3.11 or newer.

```powershell
git clone https://github.com/coreylallojr/localvoice.git
cd localvoice
.\install.ps1
.\run.ps1
```

You can also download the packaged Windows build from the repository's Releases page and run `LocalVoice.exe`; Python is not required for that build.

For system-wide microphone use, install a virtual audio cable. Select its playback/input endpoint as LocalVoice's **Output / cable**, then select the cable's recording/output endpoint as the microphone in Discord, OBS, a browser, or a game.

## Hotkeys

| Action | Shortcut |
| --- | --- |
| Start/stop | Ctrl+Shift+Space |
| Mute | Ctrl+Shift+M |
| Bypass effects | Ctrl+Shift+B |
| Previous/next character | Ctrl+Shift+Left/Right |

## Development

```powershell
.\.venv\Scripts\python -m unittest discover -v
.\build.ps1
```

## Current boundary

v0.1 is the useful local DSP release. RVC/ONNX model inference, local voice training, a bundled signed virtual microphone driver, monitoring, and installer packaging are tracked as follow-up releases. A Windows kernel audio driver must be signed before it can be safely distributed to ordinary users.

LocalVoice is GPL-3.0-only because its live pitch-shifting dependency includes GPL components.

