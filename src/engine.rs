//! Real-time audio: mic → ring buffer → voice chain → virtual mic (+ headphones).
//!
//! Streams live on one control thread, so hotkeys and the tray can start, stop
//! and switch voices while the window is hidden and a game has focus.

use crate::devices;
use crate::dsp::{Chain, EffectSwitch};
use crate::voices::Params;
use cpal::traits::{DeviceTrait, StreamTrait};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Largest callback we process in one piece; bigger callbacks are chunked.
const CHUNK: usize = 4096;

#[derive(Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(value: f32) -> Self {
        Self(AtomicU32::new(value.to_bits()))
    }
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Relaxed))
    }
    pub fn set(&self, value: f32) {
        self.0.store(value.to_bits(), Relaxed);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Stopped,
    /// Changed voice to the virtual mic; headphones optional.
    Live,
    /// Changed voice to headphones only; nothing reaches other apps.
    Test,
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub mode: Mode,
    pub rate: u32,
    pub message: String,
    pub is_error: bool,
}

/// Lock-free state shared by the UI, hotkeys and audio callbacks.
pub struct Shared {
    pub muted: AtomicBool,
    pub bypass: AtomicBool,
    pub monitor_on: AtomicBool,
    pub input_gain: AtomicF32,
    pub monitor_volume: AtomicF32,
    pub input_level: AtomicF32,
    pub output_level: AtomicF32,
    pub load: AtomicF32,
    pub dropouts: AtomicU32,
    device_lost: AtomicBool,
    pending: Mutex<Option<Params>>,
    pub status: Mutex<Status>,
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            muted: AtomicBool::new(false),
            bypass: AtomicBool::new(false),
            monitor_on: AtomicBool::new(false),
            input_gain: AtomicF32::new(1.0),
            monitor_volume: AtomicF32::new(0.8),
            input_level: AtomicF32::default(),
            output_level: AtomicF32::default(),
            load: AtomicF32::default(),
            dropouts: AtomicU32::new(0),
            device_lost: AtomicBool::new(false),
            pending: Mutex::new(None),
            status: Mutex::new(Status::default()),
        })
    }

    /// Live voice change: picked up by the audio callback on its next block.
    pub fn set_params(&self, params: Params) {
        *self.pending.lock().unwrap() = Some(params);
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    fn set_status(&self, mode: Mode, rate: u32, message: impl Into<String>, is_error: bool) {
        *self.status.lock().unwrap() = Status {
            mode,
            rate,
            message: message.into(),
            is_error,
        };
    }
}

/// Approximate mic-to-app latency: WASAPI periods, our jitter buffer, pitch FFT.
pub fn latency_ms(params: &Params) -> f32 {
    let shifting = params.pitch_semitones != 0.0 || params.formant_semitones != 0.0;
    let denoise = if params.denoise { 10.0 } else { 0.0 };
    let pitch = if shifting {
        crate::dsp::pitch::LATENCY as f32 / 48.0
    } else {
        0.0
    };
    20.0 + 12.0 + denoise + pitch
}

pub struct StartRequest {
    pub mode: Mode,
    pub input: String,
    pub virtual_mic: Option<String>,
    pub headphones: Option<String>,
    pub params: Params,
    /// Self-test: replace the mic with a 220 Hz tone (used by `--soak`).
    pub test_tone: bool,
}

pub enum Command {
    Start(StartRequest),
    Stop,
    Quit,
}

struct Streams {
    _streams: Vec<cpal::Stream>,
}

pub fn spawn(shared: Arc<Shared>, notify: Arc<dyn Fn() + Send + Sync>) -> Sender<Command> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("localvoice-audio-control".into())
        .spawn(move || control_loop(shared, receiver, notify))
        .expect("spawn audio control thread");
    sender
}

fn control_loop(
    shared: Arc<Shared>,
    receiver: Receiver<Command>,
    notify: Arc<dyn Fn() + Send + Sync>,
) {
    let mut running: Option<Streams> = None;
    loop {
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(Command::Start(request)) => {
                running = None; // Close old streams before opening devices again.
                let mode = request.mode;
                match open(&shared, request) {
                    Ok((streams, rate)) => {
                        running = Some(streams);
                        let message = match mode {
                            Mode::Live => "LIVE - your changed voice is on the virtual mic.",
                            _ => "HEADPHONE TEST - nothing is sent to other apps.",
                        };
                        shared.set_status(mode, rate, message, false);
                    }
                    Err(error) => shared.set_status(Mode::Stopped, 0, error, true),
                }
                notify();
            }
            Ok(Command::Stop) => {
                running = None;
                shared.set_status(Mode::Stopped, 0, "Stopped.", false);
                notify();
            }
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {
                if running.is_some() && shared.device_lost.swap(false, Relaxed) {
                    running = None;
                    shared.set_status(
                        Mode::Stopped,
                        0,
                        "An audio device was disconnected. Check your devices and press GO LIVE again.",
                        true,
                    );
                    notify();
                }
            }
        }
        if running.is_none() {
            shared.input_level.set(0.0);
            shared.output_level.set(0.0);
        }
    }
}

fn error_callback(shared: &Arc<Shared>) -> impl FnMut(cpal::Error) + Send + 'static {
    let shared = shared.clone();
    move |error| {
        let text = error.to_string().to_lowercase();
        if text.contains("not available")
            || text.contains("disconnected")
            || text.contains("invalidated")
        {
            shared.device_lost.store(true, Relaxed);
        }
    }
}

/// Pops `out.len()` samples with clock-drift control. Returns false on underrun.
struct Reader {
    consumer: HeapCons<f32>,
    target: usize,
    ceiling: usize,
    primed: bool,
}

impl Reader {
    fn read(&mut self, out: &mut [f32]) -> bool {
        let available = self.consumer.occupied_len();
        if !self.primed {
            out.fill(0.0);
            self.primed = available >= out.len() + self.target;
            return true;
        }
        // Mic and output clocks drift apart; drop the excess instead of letting latency grow.
        if available > out.len() + self.ceiling {
            self.consumer.skip(available - out.len() - self.target);
        }
        let got = self.consumer.pop_slice(out);
        if got < out.len() {
            out[got..].fill(0.0);
            self.primed = false;
            return false;
        }
        true
    }
}

fn open(shared: &Arc<Shared>, request: StartRequest) -> Result<(Streams, u32), String> {
    let mic = devices::find_input(&request.input).ok_or_else(|| {
        format!(
            "Microphone \"{}\" was not found. Click Refresh.",
            request.input
        )
    })?;
    let mic_config = mic
        .default_input_config()
        .map_err(|e| format!("Microphone: {e}"))?;
    let rate = mic_config.sample_rate();
    let mic_channels = mic_config.channels() as usize;

    let (primary_name, monitor_name) = match request.mode {
        Mode::Live => (
            request
                .virtual_mic
                .clone()
                .ok_or("Install VB-CABLE to go live (click Set up).")?,
            request.headphones.clone(),
        ),
        _ => (
            request
                .headphones
                .clone()
                .ok_or("Choose your headphones first.")?,
            None,
        ),
    };
    let primary = devices::find_output(&primary_name)
        .ok_or_else(|| format!("Output \"{primary_name}\" was not found. Click Refresh."))?;
    let monitor = match &monitor_name {
        Some(name) if *name != primary_name => devices::find_output(name),
        _ => None,
    };

    let target = (rate as f32 * 0.012) as usize;
    let ceiling = (rate as f32 * 0.05) as usize;
    let (mut mic_producer, mic_consumer) = HeapRb::<f32>::new(rate as usize).split();
    let (mut monitor_producer, monitor_consumer): (HeapProd<f32>, HeapCons<f32>) =
        HeapRb::<f32>::new(rate as usize).split();

    shared.dropouts.store(0, Relaxed);
    shared.device_lost.store(false, Relaxed);
    // Mic → ring buffer (mono, input gain, level meter).
    let input_shared = shared.clone();
    let mut mono = vec![0.0f32; CHUNK];
    let test_tone = request.test_tone;
    let tone_step = 2.0 * std::f32::consts::PI * 220.0 / rate as f32;
    let mut tone_phase = 0.0f32;
    let input_stream = mic
        .build_input_stream::<f32, _, _>(
            cpal::StreamConfig {
                channels: mic_channels as u16,
                sample_rate: rate,
                buffer_size: cpal::BufferSize::Default,
            },
            move |data: &[f32], _| {
                let gain = input_shared.input_gain.get();
                let mut peak = 0.0f32;
                for frames in data.chunks(CHUNK * mic_channels) {
                    let count = frames.len() / mic_channels;
                    for (i, frame) in frames.chunks_exact(mic_channels).enumerate() {
                        let mut sample = frame.iter().sum::<f32>() / mic_channels as f32 * gain;
                        if test_tone {
                            tone_phase = (tone_phase + tone_step) % (2.0 * std::f32::consts::PI);
                            sample = 0.3 * tone_phase.sin();
                        }
                        peak = peak.max(sample.abs());
                        mono[i] = sample;
                    }
                    mic_producer.push_slice(&mono[..count]);
                }
                input_shared.input_level.set(peak);
            },
            error_callback(shared),
            None,
        )
        .map_err(|e| format!("Could not open the microphone: {e}"))?;

    // Ring buffer → voice chain → primary output (virtual mic, or headphones in Test mode).
    let primary_channels = primary
        .default_output_config()
        .map_err(|e| format!("Output: {e}"))?
        .channels() as usize;
    let mode = request.mode;
    let has_monitor = monitor.is_some();
    let output_shared = shared.clone();
    let mut chain = Chain::new(&request.params, rate);
    let mut reader = Reader {
        consumer: mic_consumer,
        target,
        ceiling,
        primed: false,
    };
    let mut block = vec![0.0f32; CHUNK];
    let mut failed = false;
    let mut switch = EffectSwitch::new(rate, !shared.bypass.load(Relaxed));
    let primary_stream = primary
        .build_output_stream::<f32, _, _>(
            cpal::StreamConfig {
                channels: primary_channels as u16,
                sample_rate: rate,
                buffer_size: cpal::BufferSize::Default,
            },
            move |data: &mut [f32], _| {
                let started = Instant::now();
                if let Ok(mut pending) = output_shared.pending.try_lock()
                    && let Some(params) = pending.take()
                {
                    chain.set(&params);
                    failed = false;
                }
                let muted = output_shared.muted.load(Relaxed);
                let bypass = output_shared.bypass.load(Relaxed);
                let monitoring = has_monitor && output_shared.monitor_on.load(Relaxed);
                let volume = output_shared.monitor_volume.get();
                let mut peak = 0.0f32;
                for frames in data.chunks_mut(CHUNK * primary_channels) {
                    let count = frames.len() / primary_channels;
                    let samples = &mut block[..count];
                    if !reader.read(samples) {
                        output_shared.dropouts.fetch_add(1, Relaxed);
                    }
                    if !failed {
                        // Fail silent: a DSP bug must never leak the raw voice.
                        failed = !catch_unwind(AssertUnwindSafe(|| {
                            switch.process(&mut chain, samples, !bypass)
                        }))
                        .unwrap_or(false);
                    }
                    if failed {
                        samples.fill(0.0);
                    }
                    for (frame, &sample) in frames
                        .chunks_exact_mut(primary_channels)
                        .zip(samples.iter())
                    {
                        peak = peak.max(sample.abs());
                        let value = match mode {
                            Mode::Test => sample * volume,
                            _ if muted => 0.0,
                            _ => sample,
                        };
                        frame.fill(value);
                    }
                    if monitoring {
                        monitor_producer.push_slice(samples);
                    }
                }
                output_shared.output_level.set(peak);
                let budget = data.len() as f32 / primary_channels as f32 / rate as f32;
                output_shared
                    .load
                    .set(started.elapsed().as_secs_f32() / budget.max(1e-6));
            },
            error_callback(shared),
            None,
        )
        .map_err(|e| format!("Could not open \"{primary_name}\": {e}"))?;

    let mut streams = vec![input_stream, primary_stream];

    // Monitor ring → headphones while live.
    if let Some(monitor) = monitor {
        let channels = monitor
            .default_output_config()
            .map(|c| c.channels() as usize)
            .unwrap_or(2);
        let monitor_shared = shared.clone();
        let mut reader = Reader {
            consumer: monitor_consumer,
            target,
            ceiling,
            primed: false,
        };
        let mut block = vec![0.0f32; CHUNK];
        let stream = monitor
            .build_output_stream::<f32, _, _>(
                cpal::StreamConfig {
                    channels: channels as u16,
                    sample_rate: rate,
                    buffer_size: cpal::BufferSize::Default,
                },
                move |data: &mut [f32], _| {
                    let on = monitor_shared.monitor_on.load(Relaxed);
                    let volume = monitor_shared.monitor_volume.get();
                    for frames in data.chunks_mut(CHUNK * channels) {
                        let samples = &mut block[..frames.len() / channels];
                        reader.read(samples);
                        for (frame, &sample) in
                            frames.chunks_exact_mut(channels).zip(samples.iter())
                        {
                            frame.fill(if on { sample * volume } else { 0.0 });
                        }
                    }
                },
                error_callback(shared),
                None,
            )
            .map_err(|e| format!("Could not open your headphones: {e}"))?;
        streams.push(stream);
    }

    for stream in &streams {
        stream
            .play()
            .map_err(|e| format!("Could not start audio: {e}"))?;
    }
    Ok((Streams { _streams: streams }, rate))
}
