//! `LocalVoice.exe --soak <seconds>`: headless real-device self-test.
//!
//! Feeds a 220 Hz tone through the live engine into the virtual mic, records the
//! far side of the cable ("CABLE Output"), and checks for dropouts, DSP load,
//! and that the pitch the cable carries is the one the voice asked for.

use crate::app::Controller;
use crate::engine::Mode;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn say(line: &str) {
    use std::io::Write;
    // SAFETY: attaching to the parent console so a GUI-subsystem exe can print.
    unsafe {
        let _ = windows::Win32::System::Console::AttachConsole(
            windows::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
    }
    println!("{line}");
    let _ = std::io::stdout().flush();
    let log = crate::voices::data_dir().join("soak.log");
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .and_then(|mut f| writeln!(f, "{line}"));
}

type Recording = (cpal::Stream, Arc<Mutex<Vec<f32>>>, u32);

fn record_cable() -> Option<Recording> {
    let host = cpal::default_host();
    let device = host.input_devices().ok()?.find(|d| {
        d.description()
            .map(|x| x.name().to_lowercase().starts_with("cable output"))
            .unwrap_or(false)
    })?;
    let config = device.default_input_config().ok()?;
    let channels = config.channels() as usize;
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let sink = buffer.clone();
    let stream = device
        .build_input_stream::<f32, _, _>(
            config.config(),
            move |data: &[f32], _| {
                let mut sink = sink.lock().unwrap();
                sink.extend(data.chunks_exact(channels).map(|frame| frame[0]));
            },
            |_| {},
            None,
        )
        .ok()?;
    stream.play().ok()?;
    Some((stream, buffer, config.sample_rate()))
}

fn peak_hz(signal: &[f32], rate: u32) -> f32 {
    let mut planner = realfft::RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(signal.len());
    let mut input = signal.to_vec();
    let mut output = fft.make_output_vec();
    let _ = fft.process(&mut input, &mut output);
    let (bin, _) = output
        .iter()
        .enumerate()
        .skip(2)
        .max_by(|a, b| a.1.norm().total_cmp(&b.1.norm()))
        .unwrap_or((0, &Default::default()));
    bin as f32 * rate as f32 / signal.len() as f32
}

/// `--self-check`: packaged-exe smoke test that needs no GPU or audio hardware
/// (used by CI). Loads voices and settings, lists devices, runs every voice.
pub fn self_check() -> i32 {
    let voices = crate::voices::Store::new(&crate::voices::data_dir()).load();
    let lists = crate::devices::list();
    for voice in &voices {
        let mut chain = crate::dsp::Chain::new(&voice.params, 48_000);
        let mut block: Vec<f32> = (0..48_000).map(|i| 0.2 * (i as f32 * 0.03).sin()).collect();
        for chunk in block.chunks_mut(480) {
            chain.process(chunk);
        }
        if !block.iter().all(|s| s.is_finite()) {
            say(&format!("FAIL voice {} produced non-finite audio", voice.name));
            return 1;
        }
    }
    say(&format!(
        "self-check OK: {} voices, {} inputs, {} virtual mics, {} outputs",
        voices.len(),
        lists.inputs.len(),
        lists.virtual_outputs.len(),
        lists.monitors.len()
    ));
    0
}

pub fn run(seconds: u64) -> i32 {
    let controller = Controller::new(Arc::new(|| {}));
    let shared = controller.shared.clone();
    let (input, output, monitor, voices) = {
        let session = controller.session();
        let s = &session.settings;
        (
            s.input.clone(),
            s.output.clone(),
            s.monitor.clone(),
            session.voices.clone(),
        )
    };
    let original_voice = controller.session().current;
    let original_monitor = controller.session().settings.monitor_enabled;
    say(&format!(
        "LocalVoice soak: mic \"{input}\" -> \"{output}\", headphones \"{monitor}\", {seconds}s"
    ));
    let mut failures = Vec::new();

    // 1. Pitch check through the real virtual cable with a test tone and the Deep voice.
    let deep = voices.iter().position(|v| v.id == "deep").unwrap_or(0);
    controller.select_voice(deep);
    let mut request = controller.request(Mode::Live);
    request.test_tone = true;
    request.headphones = None;
    shared.muted.store(false, Relaxed);
    shared.bypass.store(false, Relaxed);
    controller.start(request);
    std::thread::sleep(Duration::from_millis(600));
    let status = shared.status();
    if status.mode != Mode::Live {
        say(&format!("FAIL could not go live: {}", status.message));
        return 1;
    }
    match record_cable() {
        Some((stream, buffer, rate)) => {
            std::thread::sleep(Duration::from_millis(1500));
            drop(stream);
            let samples = buffer.lock().unwrap().clone();
            let tail = &samples[samples.len().saturating_sub(rate as usize)..];
            let rms = (tail.iter().map(|x| x * x).sum::<f32>() / tail.len().max(1) as f32).sqrt();
            let found = peak_hz(tail, rate);
            let expected = 220.0 * 2f32.powf(voices[deep].params.pitch_semitones / 12.0);
            say(&format!(
                "cable loopback: rms {rms:.3}, pitch {found:.1} Hz (expected {expected:.1} Hz)"
            ));
            if rms < 0.01 || (found / expected - 1.0).abs() > 0.02 {
                failures.push("virtual mic did not carry the pitch-shifted voice".to_string());
            }
        }
        None => failures.push("could not record CABLE Output".to_string()),
    }
    // Same tone with effects bypassed: 0.212 RMS means the cable passes level 1:1.
    shared.bypass.store(true, Relaxed);
    if let Some((stream, buffer, rate)) = record_cable() {
        std::thread::sleep(Duration::from_millis(1000));
        drop(stream);
        let samples = buffer.lock().unwrap().clone();
        let tail = &samples[samples.len().saturating_sub(rate as usize / 2)..];
        let rms = (tail.iter().map(|x| x * x).sum::<f32>() / tail.len().max(1) as f32).sqrt();
        say(&format!(
            "cable level check (bypass, sent 0.212 rms): received {rms:.3}"
        ));
    }
    shared.bypass.store(false, Relaxed);

    // 2. Real mic soak with headphone monitor, cycling voices, mute, bypass, PTT.
    controller.stop();
    std::thread::sleep(Duration::from_millis(300));
    shared.monitor_on.store(true, Relaxed);
    let previous_volume = shared.monitor_volume.get();
    shared.monitor_volume.set(0.0); // Silent: the soak must not blast the user's ears.
    controller.go_live();
    std::thread::sleep(Duration::from_millis(600));
    shared.dropouts.store(0, Relaxed);
    let started = Instant::now();
    let mut max_load = 0.0f32;
    let mut step = 0;
    while started.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(Duration::from_millis(100));
        max_load = max_load.max(shared.load.get());
        if started.elapsed() > Duration::from_millis(step * 1500) {
            controller.select_voice(step as usize % voices.len());
            match step % 4 {
                1 => controller.toggle_mute(),
                2 => controller.toggle_bypass(),
                3 => {
                    controller.toggle_mute();
                    controller.toggle_bypass();
                }
                _ => {}
            }
            step += 1;
        }
        if shared.status().mode != Mode::Live {
            failures.push(format!("engine stopped: {}", shared.status().message));
            break;
        }
    }
    let dropouts = shared.dropouts.load(Relaxed);
    controller.stop();
    shared.monitor_volume.set(previous_volume);
    std::thread::sleep(Duration::from_millis(300));
    say(&format!(
        "soak: {step} voice/mute/bypass changes, {dropouts} dropouts, peak DSP load {:.1}%",
        max_load * 100.0
    ));
    if dropouts > 0 {
        failures.push(format!("{dropouts} dropouts"));
    }
    if max_load > 0.5 {
        failures.push(format!("DSP load too high: {:.0}%", max_load * 100.0));
    }
    controller.select_voice(original_voice);
    controller.session().settings.monitor_enabled = original_monitor;
    controller.quit();
    if failures.is_empty() {
        say("PASS");
        0
    } else {
        for failure in &failures {
            say(&format!("FAIL {failure}"));
        }
        1
    }
}
