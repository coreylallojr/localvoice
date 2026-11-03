# LocalVoice repository guidance

LocalVoice is a Windows-first, fully local voice changer written in Rust. The live product must remain useful without an account, telemetry, cloud inference, or a network connection.

## Working agreements

- Keep audio callbacks real-time safe: no allocation, file or network access, blocking locks (only `try_lock`), model loading or UI work. Size buffers up front (`Chain::new`, `EffectSwitch::new`).
- Fail silent on processing errors (panics, NaN) so a user's unmodified voice is not leaked unexpectedly.
- All behaviour goes through `app::Controller`, so the UI, hotkeys and tray act identically, including while the window is hidden and a game has focus.
- Push-to-talk controls the voice effect (bypass), never the microphone. Mute is a separate control.
- Treat imported packs, models and media as untrusted. Validate archive paths and sizes; prefer ONNX or Safetensors.
- Preserve the simple path for non-technical users: one executable, sensible device defaults, headphone testing and global controls.
- New voice engines must implement a common streaming boundary rather than adding engine-specific behavior to the UI.
- Never bundle a voice model or dataset without verified redistribution and voice-use rights.

## Verification

Run before shipping:

```powershell
cargo clippy --all-targets -- -D warnings
cargo test --release
cargo run --release -- --soak 30   # needs a mic + VB-CABLE; must print PASS
.\install.ps1                      # then launch with `localvoice`
```

For audio changes, also exercise the following by hand:
- microphone capture
- virtual output (record CABLE Output)
- headphone monitoring
- mute
- effect on/off
- push-to-talk hold and toggle
- voice switching
- device unplug

Read `PLANS.md` before beginning an advanced voice-model phase.
