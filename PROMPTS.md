# Prompts for future sessions

Run from the repository root: `C:\Users\Corey\Documents\localvoice`.

## Next implementation prompt

```text
Continue LocalVoice from the repository root. Read AGENTS.md, PLANS.md, README.md, the current tests, and the live implementation before changing anything.

Implement Phase 2: the local media-ingestion and dataset-workbench foundation. Start with the smallest complete vertical slice: select one or more supported media files, use a locally discovered or bundled FFmpeg/ffprobe to extract mono WAV safely, analyze duration/sample rate/channels/peak/RMS/clipping/silence, show the results in the desktop UI, and write a reproducible dataset manifest without modifying source files.

Keep ingestion out of the real-time audio callback. Treat filenames, containers, archives, and subprocess output as untrusted. Use explicit subprocess argument lists, bounded output, cancellable work, and atomic writes. Preserve all current DSP, tray, monitoring, push-to-talk voice, voice editor, packaging, and offline behavior. The app is Rust (see AGENTS.md); keep ingestion off the audio threads.

Add focused tests for media probing, safe command construction, manifest generation, cancellation/error behavior, and paths containing spaces. Run `cargo clippy --all-targets -- -D warnings`, `cargo test --release`, `--soak 30`, a UI smoke test, and any safe end-to-end media test available locally. Update PLANS.md only with observed status/evidence. Do not begin model training or choose a neural engine in this phase. Commit, push, and publish a release only if I explicitly authorize those external writes in this session.
```

