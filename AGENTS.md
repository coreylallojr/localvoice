# LocalVoice repository guidance

LocalVoice is a Windows-first, fully local voice changer. The live product must remain useful without an account, telemetry, cloud inference, or a network connection.

## Working agreements

- Keep the audio callback non-blocking. Do not perform file access, network access, model loading, UI work, or unbounded queue operations in it.
- Fail silent on processing errors so a user's unmodified voice is not leaked unexpectedly.
- Treat imported models and media as untrusted. Validate archive paths and sizes; prefer ONNX or Safetensors and restricted PyTorch loading.
- Preserve a simple path for non-technical users: one executable, first-run guidance, persisted device choices, headphone testing, and global controls.
- New voice engines must implement a common streaming boundary rather than adding engine-specific behavior to the UI.
- Never bundle a voice model or dataset without verified redistribution and voice-use rights.

## Verification

Run before shipping:

```powershell
.\.venv\Scripts\ruff check .
.\.venv\Scripts\python -m unittest discover -v
.\build.ps1
```

Also smoke-test the packaged executable and, for audio changes, exercise microphone capture, virtual output, headphone monitoring, mute, bypass, push-to-talk press/release, and device shutdown.

Read `PLANS.md` before beginning an advanced voice-model phase. Update it only when scope, ordering, or acceptance criteria materially change.

