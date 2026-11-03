# LocalVoice development plan

## Product objective

Make locally processed and locally trained character voices easy enough to use as an ordinary Windows microphone. Users should be able to test their result through headphones, switch characters and push-to-talk globally, import/export portable characters, and eventually build high-quality models from common media files.

## Phase 1 — Impressive DSP MVP

Status: Complete. v0.2 was Python; v0.3.0 rewrote it in Rust (cpal, egui).

Scope: single-file executable, persisted device setup, system tray, push-to-talk voice (hold/toggle effect switching), headphone monitoring, live-preview voice editor, formant-preserving pitch shifting, RNNoise noise removal, safe DSP voice packs, CI and public Windows releases.

Evidence (v0.3.0, this dev PC: PRO X 2 headset + VB-CABLE 3.3.1.7):
- `--soak` passes: 0 dropouts over 20 s of voice/mute/bypass switching, peak DSP load 2%.
- A test tone through CABLE Output arrives at the target pitch (Deep: 175.0 Hz measured, 174.6 Hz expected).
- Bypass passes level 1:1.

## Phase 2 — Media ingestion and dataset workbench

Status: Next.

Scope:

- Accept video and audio containers through FFmpeg: MP4, MKV, MOV, WebM, MP3, M4A, AAC, FLAC, OGG and WAV.
- Extract audio without modifying the source file, normalize formats, detect clipping/silence, and produce a quality report.
- Add optional vocal isolation, speech/music classification, VAD-based slicing, speaker segmentation and duplicate removal.
- Let the user audition, include or reject every segment before training.
- Store source provenance, consent/rights notes and processing parameters in a dataset manifest.

Acceptance: a mixed-media folder becomes a reproducible, reviewable, single-speaker dataset; bad segments are surfaced rather than silently trained; no processing requires the cloud.

## Phase 3 — Engine benchmark and streaming adapter

Status: Planned.

Benchmark current RVC/Applio, Beatrice, Seed-VC-class zero-shot, LLVC and ONNX-native options on representative NVIDIA, AMD/DirectML and CPU hardware. Measure end-to-end latency, real-time factor, speaker similarity, intelligibility, memory use and model licensing.

Define one streaming engine contract for load, warmup, process, reset and unload (today `dsp::Chain` is the only engine). Keep any Python engines in isolated worker processes and prefer ONNX Runtime (`ort` crate) for the distributable path.

Acceptance: the chosen trained and zero-shot engines meet documented quality/latency targets and can crash without taking down or leaking through the audio path.

## Phase 4 — Local custom-voice training

Status: Planned after phases 2 and 3.

Scope: guided dataset recording/import, GPU detection, training presets, checkpoints, pause/resume, validation samples, A/B comparison, automatic pitch guidance, and export to a versioned voice pack. Training must operate locally and explain expected time/disk/VRAM before starting.

Acceptance: a user can move from authorized source media to a validated character without editing configuration files or using a terminal.

## Phase 5 — Voice pack v2 and model safety

Add engine/model metadata, hashes, artwork, preview audio, attribution, rights statements, minimum hardware, default controls and migration rules. Reject traversal, decompression bombs and executable payloads. Prefer ONNX/Safetensors; quarantine legacy pickle-based checkpoints behind explicit trust warnings and restricted loading.

## Phase 6 — Native virtual microphone and installer

Replace the external cable dependency with a minimal SysVAD-derived WaveRT transport, then complete driver signing, upgrade/uninstall recovery, installer integration and Windows audio compatibility testing. Keep all inference in user space.

## Explicit non-goals

- Cloud-only inference or training
- Automatic scraping or cloning of voices without clear user-provided rights
- Running neural inference or complex DSP in kernel mode
- Claiming that a model is “perfect”; quality must be measured and auditioned

