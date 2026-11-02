# LocalVoice development plan

## Product objective

Make locally processed and locally trained character voices easy enough to use as an ordinary Windows microphone. Users should be able to test their result through headphones, switch characters and push-to-talk globally, import/export portable characters, and eventually build high-quality models from common media files.

## Phase 1 — Impressive DSP MVP

Status: Complete in v0.2.

Scope: single-file launcher, persisted device setup, system tray, configurable hold/toggle push-to-talk, headphone monitoring, character editor, safe DSP voice packs, CI and public Windows releases.

Acceptance: clean-machine package launches without Python; live audio and separate monitoring work at 48 kHz; PTT never leaks dry audio; settings survive restart.

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

Define one streaming engine contract for load, warmup, process, reset and unload. Keep Python engines in isolated workers and prefer ONNX Runtime for the distributable path.

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

