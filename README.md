# LocalVoice

LocalVoice is a fully local, real-time Windows voice changer. It captures a physical microphone, applies a character preset, and sends the result to any output device. Point it at a virtual audio cable and every application can use the changed voice as an ordinary microphone.

No account, cloud service, telemetry, or network connection is used while running.

## What works in v0.2.2

- Live microphone processing with eight bundled characters
- Pitch shift, gate, EQ, compression, distortion, bit crushing, chorus and reverb
- Configurable push to talk with hold and toggle modes
- Global hotkeys for live/stop, mute, bypass and character switching
- One-click headphone testing that works before a virtual microphone is installed
- System-tray controls and duplicate-instance protection
- First-run guidance and persistent device/hotkey settings
- Full custom character editor for pitch, gate, tone, grit, bit depth, chorus, reverb and gain
- Safe `.voicepack` import and export
- Input/output meters and fail-silent audio errors
- Any Windows input/output device supported by PortAudio/WASAPI

## Easiest way to run it

1. Download `LocalVoice-windows-x64.exe` from the [latest release](https://github.com/coreylallojr/localvoice/releases/latest).
2. Double-click it. Windows may show a SmartScreen warning because the free build is not code-signed; choose **More info → Run anyway** if you downloaded it from this repository.
3. Pick your physical microphone and headphones, then click **TEST IN HEADPHONES**. This works without a virtual driver. Wear headphones to prevent feedback.
4. To use the changed voice in other apps, install [VB-CABLE from its official site](https://vb-audio.com/Cable/): extract its ZIP, run `VBCABLE_Setup_x64.exe` as administrator, and restart Windows.
5. Launch LocalVoice again and click **Refresh**. It will select **CABLE Input** automatically.
6. Click **GO LIVE**. In Discord, a game, OBS, or a browser, select **CABLE Output** as the microphone.

Push to talk starts off so first-run testing is not silent. Enable it when ready; F8 is the default key, and **Hotkey settings** can change the key or switch between hold and toggle behavior. Closing the window keeps LocalVoice available in the system tray; choose **Quit** in the app or tray when finished.

On this development machine, open a new Windows PowerShell window and run `localvoice` to launch the latest local build.

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

