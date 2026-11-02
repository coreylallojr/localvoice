# LocalVoice

LocalVoice is a fully local, real-time Windows voice changer. It captures a physical microphone, applies a character preset, and sends the result to any output device. Point it at a virtual audio cable and every application can use the changed voice as an ordinary microphone.

No account, cloud service, telemetry, or network connection is used while running.

## What works in v0.2

- Live microphone processing with eight bundled characters
- Pitch shift, gate, EQ, compression, distortion, bit crushing, chorus and reverb
- Configurable push to talk with hold and toggle modes
- Global hotkeys for live/stop, mute, bypass and character switching
- Separate headphone monitoring so you can hear the processed voice
- System-tray controls and duplicate-instance protection
- First-run guidance and persistent device/hotkey settings
- Full custom character editor for pitch, gate, tone, grit, bit depth, chorus, reverb and gain
- Safe `.voicepack` import and export
- Input/output meters and fail-silent audio errors
- Any Windows input/output device supported by PortAudio/WASAPI

## Easiest way to run it

1. Download `LocalVoice-windows-x64.exe` from the [latest release](https://github.com/coreylallojr/localvoice/releases/latest).
2. Double-click it. Windows may show a SmartScreen warning because the free build is not code-signed; choose **More info → Run anyway** if you downloaded it from this repository.
3. Pick your physical microphone.
4. Install the signed, open-source [Virtual Audio Driver](https://github.com/VirtualDrivers/Virtual-Audio-Driver/releases/latest), then pick its playback endpoint as **Virtual microphone**.
5. Pick your headphones and click **HEAR MY VOICE**. Wear headphones to prevent feedback.
6. Click **GO LIVE**. In Discord, a game, OBS, or a browser, select the virtual driver's recording endpoint as the microphone.

F8 is push to talk by default. Open **Hotkey settings** to choose a different key or switch between hold and toggle behavior. Closing the window keeps LocalVoice available in the system tray; choose **Quit** in the app or tray when finished.

## Run from source

Requirements: Windows 10/11 and Python 3.11 or newer.

```powershell
git clone https://github.com/coreylallojr/localvoice.git
cd localvoice
.\install.ps1
.\run.ps1
```

Python is not required for the packaged Windows executable.

## Hotkeys

| Action | Shortcut |
| --- | --- |
| Push to talk | F8 (configurable) |
| Start/stop | Ctrl+Shift+Space |
| Mute | Ctrl+Shift+M |
| Bypass effects | Ctrl+Shift+B |
| Previous/next character | Ctrl+Shift+Left/Right |

## Development

```powershell
.\.venv\Scripts\python -m unittest discover -v
.\build.ps1
```

## Advanced roadmap

The detailed plan for media ingestion, dataset preparation, engine benchmarking, local training, safer model packs, and a native signed virtual microphone is in [`PLANS.md`](PLANS.md). Future Codex sessions should start in this repository root and use [`PROMPTS.md`](PROMPTS.md).

LocalVoice is GPL-3.0-only because its live pitch-shifting dependency includes GPL components.

