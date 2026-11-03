//! The live voice chain. `set` retunes in place and `process` never allocates,
//! so sliders change the sound live without clicks or audio-thread allocation.

pub mod effects;
pub mod pitch;

use crate::voices::Params;
use biquad::Type;
use effects::*;
use nnnoiseless::DenoiseState;
use pitch::PitchShifter;

const DENOISE_FRAME: usize = nnnoiseless::FRAME_SIZE;

pub struct Chain {
    rate: f32,
    denoise: Box<DenoiseState<'static>>,
    denoise_on: bool,
    denoise_in: [f32; DENOISE_FRAME],
    denoise_out: [f32; DENOISE_FRAME],
    denoise_pos: usize,
    gate: Gate,
    highpass: Filter,
    shifter: PitchShifter,
    shifting: bool,
    ring_phase: f32,
    ring_step: f32,
    compressor: Compressor,
    presence: Filter,
    lowpass: Filter,
    drive: f32,
    drive_makeup: f32,
    crush: f32,
    chorus: Chorus,
    reverb: Reverb,
    gain: f32,
    limiter: Limiter,
}

impl Chain {
    pub fn new(params: &Params, rate: u32) -> Self {
        let rate = rate as f32;
        let mut chain = Self {
            rate,
            denoise: DenoiseState::new(),
            denoise_on: false,
            denoise_in: [0.0; DENOISE_FRAME],
            denoise_out: [0.0; DENOISE_FRAME],
            denoise_pos: 0,
            gate: Gate::new(rate),
            highpass: Filter::new(),
            shifter: PitchShifter::new(),
            shifting: false,
            ring_phase: 0.0,
            ring_step: 0.0,
            compressor: Compressor::new(rate),
            presence: Filter::new(),
            lowpass: Filter::new(),
            drive: 0.0,
            drive_makeup: 1.0,
            crush: 0.0,
            chorus: Chorus::new(rate),
            reverb: Reverb::new(rate),
            gain: 1.0,
            limiter: Limiter::new(rate),
        };
        chain.set(params);
        chain
    }

    pub fn set(&mut self, p: &Params) {
        let rate = self.rate;
        // ponytail: RNNoise is trained for 48 kHz only; other mic rates skip it.
        let denoise_on = p.denoise && (rate - 48_000.0).abs() < 1.0;
        if denoise_on && !self.denoise_on {
            // Don't replay a stale frame from the last time noise removal was on.
            self.denoise_in.fill(0.0);
            self.denoise_out.fill(0.0);
            self.denoise_pos = 0;
        }
        self.denoise_on = denoise_on;
        self.gate.set(p.gate_db);
        self.highpass.highpass(rate, p.highpass_hz);
        self.shifter.set(p.pitch_semitones, p.formant_semitones);
        let shifting = !self.shifter.is_identity();
        if shifting && !self.shifting {
            self.shifter.reset();
        }
        self.shifting = shifting;
        self.ring_step = 2.0 * std::f32::consts::PI * p.ring_hz / rate;
        self.presence.set(
            Type::PeakingEQ(p.presence_db),
            rate,
            3_000.0,
            0.8,
            p.presence_db != 0.0,
        );
        self.lowpass.set(
            Type::LowPass,
            rate,
            p.lowpass_hz,
            0.707,
            p.lowpass_hz < 19_999.0,
        );
        self.drive = db_to_gain(p.distortion_db);
        self.drive_makeup = 1.0 / self.drive.sqrt();
        self.crush = if p.bit_depth < 31.5 {
            2f32.powf(p.bit_depth - 1.0)
        } else {
            0.0
        };
        self.chorus.mix = p.chorus_mix;
        self.reverb.mix = p.reverb_mix;
        self.gain = db_to_gain(p.gain_db);
    }

    /// Samples of delay the chain adds (noise removal frame + pitch FFT).
    pub fn latency(&self) -> usize {
        let denoise = if self.denoise_on { DENOISE_FRAME } else { 0 };
        let pitch = if self.shifting { pitch::LATENCY } else { 0 };
        denoise + pitch
    }

    pub fn process(&mut self, block: &mut [f32]) {
        for x in block.iter_mut() {
            let mut s = *x;
            if self.denoise_on {
                self.denoise_in[self.denoise_pos] = s * 32_768.0;
                s = self.denoise_out[self.denoise_pos] / 32_768.0;
                self.denoise_pos += 1;
                if self.denoise_pos == DENOISE_FRAME {
                    self.denoise_pos = 0;
                    self.denoise
                        .process_frame(&mut self.denoise_out, &self.denoise_in);
                }
            }
            s = self.gate.run(s);
            *x = self.highpass.run(s);
        }
        if self.shifting {
            self.shifter.process(block);
        }
        for x in block.iter_mut() {
            let mut s = *x;
            if self.ring_step > 0.0 {
                self.ring_phase = (self.ring_phase + self.ring_step) % (2.0 * std::f32::consts::PI);
                s *= self.ring_phase.sin();
            }
            s = self.compressor.run(s);
            s = self.presence.run(s);
            s = self.lowpass.run(s);
            if self.drive > 1.001 {
                s = (s * self.drive).tanh() * self.drive_makeup;
            }
            if self.crush > 0.0 {
                s = (s * self.crush).round() / self.crush;
            }
            if self.chorus.mix > 0.0 {
                s = self.chorus.run(s);
            }
            if self.reverb.mix > 0.0 {
                s = self.reverb.run(s);
            }
            *x = self.limiter.run(s * self.gain);
        }
    }
}

/// Voice effect on/off (push-to-talk, bypass). The chain always runs so it stays
/// warm; the output crossfades (15 ms) against a dry copy delayed by the chain's
/// latency, so switching never clicks or jumps in time.
pub struct EffectSwitch {
    dry: Vec<f32>,
    position: usize,
    wet: f32,
    fade: f32,
    /// Dry tap delay; when the chain's latency changes (voice switch), the old
    /// tap fades into the new one instead of skipping or repeating audio.
    delay: usize,
    old_delay: usize,
    delay_blend: f32,
}

impl EffectSwitch {
    /// Must exceed the largest block plus the chain's latency.
    const HISTORY: usize = 16_384;

    pub fn new(rate: u32, effect_on: bool) -> Self {
        Self {
            dry: vec![0.0; Self::HISTORY],
            position: 0,
            wet: if effect_on { 1.0 } else { 0.0 },
            fade: 1.0 - (-1.0 / (0.015 * rate as f32)).exp(),
            delay: 0,
            old_delay: 0,
            delay_blend: 1.0,
        }
    }

    /// Processes `block` in place. Returns false if the chain produced non-finite audio.
    pub fn process(&mut self, chain: &mut Chain, block: &mut [f32], effect_on: bool) -> bool {
        let size = Self::HISTORY;
        let start = self.position;
        for &sample in block.iter() {
            self.dry[self.position] = sample;
            self.position = (self.position + 1) % size;
        }
        chain.process(block);
        if !block.iter().all(|s| s.is_finite()) {
            return false;
        }
        let delay = chain.latency();
        if delay != self.delay {
            self.old_delay = self.delay;
            self.delay = delay;
            self.delay_blend = 0.0;
        }
        let target = if effect_on { 1.0 } else { 0.0 };
        for (i, sample) in block.iter_mut().enumerate() {
            self.wet += (target - self.wet) * self.fade;
            let index = start + i + size;
            let mut dry = self.dry[(index - self.delay) % size];
            if self.delay_blend < 1.0 {
                self.delay_blend = (self.delay_blend + self.fade).min(1.0);
                let old = self.dry[(index - self.old_delay) % size];
                dry = old + (dry - old) * self.delay_blend;
            }
            *sample = *sample * self.wet + dry * (1.0 - self.wet);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voices::builtins;
    use std::time::Instant;

    /// Speech-like test signal: a 120 Hz harmonic tone with vibrato and syllable bursts.
    fn voice_like(seconds: f32) -> Vec<f32> {
        let rate = 48_000.0;
        let mut phase = 0.0f32;
        (0..(rate * seconds) as usize)
            .map(|i| {
                let t = i as f32 / rate;
                phase += 2.0
                    * std::f32::consts::PI
                    * 120.0
                    * (1.0 + 0.03 * (2.0 * std::f32::consts::PI * 5.0 * t).sin())
                    / rate;
                let syllable = (2.0 * std::f32::consts::PI * 3.0 * t).sin().max(0.0);
                (1..20)
                    .map(|h| (h as f32 * phase).sin() / h as f32)
                    .sum::<f32>()
                    * 0.15
                    * syllable
            })
            .collect()
    }

    #[test]
    fn every_builtin_streams_full_length_and_audible() {
        for voice in builtins() {
            let mut chain = Chain::new(&voice.params, 48_000);
            let mut signal = voice_like(2.0);
            let length = signal.len();
            for block in signal.chunks_mut(480) {
                chain.process(block);
            }
            assert_eq!(signal.len(), length);
            assert!(
                signal.iter().all(|x| x.is_finite() && x.abs() <= 1.0),
                "{}",
                voice.name
            );
            let rms = (signal[48_000..].iter().map(|x| x * x).sum::<f32>() / 48_000.0).sqrt();
            assert!(rms > 0.02, "{} is too quiet: {rms}", voice.name);
        }
    }

    #[test]
    fn every_builtin_is_fast_enough_for_realtime() {
        // 480 samples = 10 ms at 48 kHz. Debug builds are far slower, so only
        // the release build is held to the 2 ms budget. p99, not max: a single
        // block pre-empted by the OS (parallel tests) is not DSP cost.
        // Shared CI runners are slower and noisier than a gaming PC.
        let ci = std::env::var_os("CI").is_some();
        let budget_ms = if cfg!(debug_assertions) { 10.0 } else if ci { 5.0 } else { 2.0 };
        for voice in builtins() {
            let mut chain = Chain::new(&voice.params, 48_000);
            let mut signal = voice_like(2.0);
            let mut times: Vec<f64> = signal
                .chunks_mut(480)
                .skip(1)
                .map(|block| {
                    let start = Instant::now();
                    chain.process(block);
                    start.elapsed().as_secs_f64() * 1_000.0
                })
                .collect();
            times.sort_by(f64::total_cmp);
            let p99 = times[times.len() * 99 / 100];
            assert!(p99 < budget_ms, "{} p99 block {p99:.3} ms", voice.name);
        }
    }

    #[test]
    fn effect_switch_is_click_free_and_bypass_is_the_real_voice() {
        let voices = builtins();
        let deep = &voices.iter().find(|v| v.id == "deep").unwrap().params;
        let mut chain = Chain::new(deep, 48_000);
        let mut switch = EffectSwitch::new(48_000, false);
        let rate = 48_000.0;
        let input: Vec<f32> = (0..96_000)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / rate).sin())
            .collect();
        let mut output = input.clone();
        // Push-to-talk pressed for the middle 0.5 s.
        for (index, block) in output.chunks_mut(480).enumerate() {
            let effect_on = (100..150).contains(&index);
            assert!(switch.process(&mut chain, block, effect_on));
        }
        // Bypassed: exactly the real voice, delayed by the chain latency.
        let delay = chain.latency();
        for i in 20_000..40_000 {
            assert!(
                (output[i] - input[i - delay]).abs() < 1e-4,
                "dry mismatch at {i}"
            );
        }
        // No clicks: the steepest step stays near the steepest step of a clean sine.
        let max_step = output
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        let sine_step = 0.3 * 2.0 * std::f32::consts::PI * 220.0 / rate;
        assert!(
            max_step < sine_step * 1.6,
            "click: step {max_step} vs sine {sine_step}"
        );
    }

    #[test]
    fn switching_voices_in_real_voice_mode_is_click_free() {
        // Clean → Deep changes the chain latency (480 → 2016 samples); the dry
        // tap must glide, not jump, or the real voice skips ~32 ms.
        let voices = builtins();
        let clean = voices.iter().find(|v| v.id == "clean").unwrap().params;
        let deep = voices.iter().find(|v| v.id == "deep").unwrap().params;
        let mut chain = Chain::new(&clean, 48_000);
        let mut switch = EffectSwitch::new(48_000, false);
        let rate = 48_000.0;
        let mut output: Vec<f32> = (0..96_000)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / rate).sin())
            .collect();
        for (index, block) in output.chunks_mut(480).enumerate() {
            if index == 100 {
                chain.set(&deep);
            }
            assert!(switch.process(&mut chain, block, false));
        }
        let max_step = output[4_800..].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        let sine_step = 0.3 * 2.0 * std::f32::consts::PI * 220.0 / rate;
        assert!(max_step < sine_step * 1.6, "jump: step {max_step} vs sine {sine_step}");
    }

    #[test]
    fn retuning_live_keeps_output_finite() {
        let voices = builtins();
        let mut chain = Chain::new(&voices[0].params, 48_000);
        let mut signal = voice_like(2.0);
        for (index, block) in signal.chunks_mut(480).enumerate() {
            chain.set(&voices[index % voices.len()].params);
            chain.process(block);
        }
        assert!(signal.iter().all(|x| x.is_finite()));
    }
}
