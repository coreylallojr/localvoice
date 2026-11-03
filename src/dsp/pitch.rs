//! Streaming phase-vocoder pitch shifter with formant control.
//!
//! Based on the classic smbPitchShift structure (2048-point FFT, 4x overlap),
//! with two additions: output bins read the source bin they came from (so peak
//! heights survive upward shifts), and a smoothed log-spectrum envelope lets the
//! formants move independently of the pitch.
//!
//! ponytail: adds ~32 ms latency. PSOLA would be lower-latency for a single
//! voice; a neural engine (PLANS phase 3) is the quality ceiling beyond this.

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::f32::consts::PI;
use std::sync::Arc;

pub const SIZE: usize = 2048;
const OVERLAP: usize = 4;
const STEP: usize = SIZE / OVERLAP;
const BINS: usize = SIZE / 2 + 1;
pub const LATENCY: usize = SIZE - STEP;
/// Envelope smoothing radius in bins (~23 Hz each at 48 kHz): wider than voice harmonics.
const ENVELOPE_RADIUS: usize = 8;

pub struct PitchShifter {
    pitch: f32,
    formant: f32,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    window: Vec<f32>,
    input: Vec<f32>,
    output: Vec<f32>,
    accumulator: Vec<f32>,
    rover: usize,
    frame: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch_forward: Vec<Complex<f32>>,
    scratch_inverse: Vec<Complex<f32>>,
    magnitude: Vec<f32>,
    true_bin: Vec<f32>,
    last_phase: Vec<f32>,
    phase_sum: Vec<f32>,
    envelope: Vec<f32>,
    smooth: Vec<f32>,
    prefix: Vec<f32>,
}

impl PitchShifter {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(SIZE);
        let inverse = planner.plan_fft_inverse(SIZE);
        let window = (0..SIZE)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / SIZE as f32).cos())
            .collect();
        Self {
            pitch: 1.0,
            formant: 1.0,
            scratch_forward: forward.make_scratch_vec(),
            scratch_inverse: inverse.make_scratch_vec(),
            spectrum: forward.make_output_vec(),
            forward,
            inverse,
            window,
            input: vec![0.0; SIZE],
            output: vec![0.0; SIZE],
            accumulator: vec![0.0; SIZE],
            rover: LATENCY,
            frame: vec![0.0; SIZE],
            magnitude: vec![0.0; BINS],
            true_bin: vec![0.0; BINS],
            last_phase: vec![0.0; BINS],
            phase_sum: vec![0.0; BINS],
            envelope: vec![0.0; BINS],
            smooth: vec![0.0; BINS],
            prefix: vec![0.0; BINS + 1],
        }
    }

    /// `formant_semitones` is relative to the original voice: 0 keeps the
    /// speaker's formants (natural), matching `pitch` gives the plain "tape" shift.
    pub fn set(&mut self, pitch_semitones: f32, formant_semitones: f32) {
        self.pitch = 2f32.powf(pitch_semitones / 12.0);
        self.formant = 2f32.powf(formant_semitones / 12.0);
    }

    /// Clears audio history (allocation-free) so a re-enabled shifter starts silent.
    pub fn reset(&mut self) {
        for buffer in [&mut self.input, &mut self.output, &mut self.accumulator] {
            buffer.fill(0.0);
        }
        self.last_phase.fill(0.0);
        self.phase_sum.fill(0.0);
        self.rover = LATENCY;
    }

    pub fn is_identity(&self) -> bool {
        (self.pitch - 1.0).abs() < 1e-4 && (self.formant - 1.0).abs() < 1e-4
    }

    pub fn process(&mut self, block: &mut [f32]) {
        for sample in block.iter_mut() {
            self.input[self.rover] = *sample;
            *sample = self.output[self.rover - LATENCY];
            self.rover += 1;
            if self.rover >= SIZE {
                self.rover = LATENCY;
                self.shift_frame();
            }
        }
    }

    fn shift_frame(&mut self) {
        let expected = 2.0 * PI * STEP as f32 / SIZE as f32;
        for i in 0..SIZE {
            self.frame[i] = self.input[i] * self.window[i];
        }
        let _ = self.forward.process_with_scratch(
            &mut self.frame,
            &mut self.spectrum,
            &mut self.scratch_forward,
        );

        for k in 0..BINS {
            let value = self.spectrum[k];
            let phase = value.im.atan2(value.re);
            let mut delta = phase - self.last_phase[k] - k as f32 * expected;
            self.last_phase[k] = phase;
            delta -= 2.0 * PI * (delta / (2.0 * PI)).round();
            self.magnitude[k] = value.norm();
            self.true_bin[k] = k as f32 + delta * OVERLAP as f32 / (2.0 * PI);
        }
        let correct_formants = (self.formant - self.pitch).abs() > 1e-4;
        if correct_formants {
            self.compute_envelope();
        }

        for j in 0..BINS {
            let source = j as f32 / self.pitch;
            let (magnitude, frequency) = if source <= (BINS - 1) as f32 {
                let low = source.floor() as usize;
                let high = (low + 1).min(BINS - 1);
                let fraction = source - low as f32;
                let mut magnitude =
                    self.magnitude[low] * (1.0 - fraction) + self.magnitude[high] * fraction;
                if correct_formants {
                    // Remove the source envelope, then apply it at the formant ratio.
                    let wanted = interpolate(&self.envelope, j as f32 / self.formant);
                    let current = interpolate(&self.envelope, source);
                    magnitude *= (wanted / current.max(1e-9)).clamp(0.05, 8.0);
                }
                let nearest = (source.round() as usize).min(BINS - 1);
                (magnitude, self.true_bin[nearest] * self.pitch)
            } else {
                (0.0, j as f32)
            };
            let advance = (frequency - j as f32) * 2.0 * PI / OVERLAP as f32 + j as f32 * expected;
            self.phase_sum[j] = (self.phase_sum[j] + advance) % (2.0 * PI);
            self.spectrum[j] = Complex::from_polar(magnitude, self.phase_sum[j]);
        }
        self.spectrum[0].im = 0.0;
        self.spectrum[BINS - 1].im = 0.0;
        let _ = self.inverse.process_with_scratch(
            &mut self.spectrum,
            &mut self.frame,
            &mut self.scratch_inverse,
        );

        // Unnormalised inverse FFT (/SIZE), Hann² overlap-add gain at 4x (/1.5).
        let gain = 1.0 / (SIZE as f32 * 1.5);
        for i in 0..SIZE {
            self.accumulator[i] += self.frame[i] * self.window[i] * gain;
        }
        self.output[..STEP].copy_from_slice(&self.accumulator[..STEP]);
        self.accumulator.copy_within(STEP..SIZE, 0);
        self.accumulator[SIZE - STEP..].fill(0.0);
        self.input.copy_within(STEP..SIZE, 0);
    }

    /// Smoothed spectral envelope: a two-pass box blur of log magnitude.
    fn compute_envelope(&mut self) {
        for k in 0..BINS {
            self.envelope[k] = (self.magnitude[k] + 1e-9).ln();
        }
        for _ in 0..2 {
            self.prefix[0] = 0.0;
            for k in 0..BINS {
                self.prefix[k + 1] = self.prefix[k] + self.envelope[k];
            }
            for k in 0..BINS {
                let low = k.saturating_sub(ENVELOPE_RADIUS);
                let high = (k + ENVELOPE_RADIUS + 1).min(BINS);
                self.smooth[k] = (self.prefix[high] - self.prefix[low]) / (high - low) as f32;
            }
            self.envelope.copy_from_slice(&self.smooth);
        }
        for value in &mut self.envelope {
            *value = value.exp();
        }
    }
}

fn interpolate(values: &[f32], position: f32) -> f32 {
    let last = values.len() - 1;
    if position >= last as f32 {
        return values[last];
    }
    let low = position.max(0.0).floor() as usize;
    let fraction = position - low as f32;
    values[low] * (1.0 - fraction) + values[low + 1] * fraction
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak_hz(signal: &[f32], rate: f32) -> f32 {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(signal.len());
        let mut input = signal.to_vec();
        let mut output = fft.make_output_vec();
        fft.process(&mut input, &mut output).unwrap();
        let (bin, _) = output
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.norm().total_cmp(&b.1.norm()))
            .unwrap();
        bin as f32 * rate / signal.len() as f32
    }

    #[test]
    fn shifts_a_tone_to_the_target_pitch() {
        let rate = 48_000.0;
        for semitones in [-10.0f32, -4.0, 3.0, 7.0] {
            for formant in [0.0, semitones] {
                let mut shifter = PitchShifter::new();
                shifter.set(semitones, formant);
                let mut signal: Vec<f32> = (0..96_000)
                    .map(|i| 0.3 * (2.0 * PI * 220.0 * i as f32 / rate).sin())
                    .collect();
                for block in signal.chunks_mut(512) {
                    shifter.process(block);
                }
                let tail = &signal[48_000..];
                let expected = 220.0 * 2f32.powf(semitones / 12.0);
                let found = peak_hz(tail, rate);
                assert!(
                    (found / expected - 1.0).abs() < 0.01,
                    "{semitones} st (formant {formant}): {found} Hz, expected {expected}"
                );
                assert!(tail.iter().all(|x| x.is_finite()));
            }
        }
    }

    #[test]
    fn unity_passes_level_through() {
        let mut shifter = PitchShifter::new();
        shifter.set(0.0, 0.0);
        let mut signal: Vec<f32> = (0..48_000)
            .map(|i| 0.3 * (2.0 * PI * 220.0 * i as f32 / 48_000.0).sin())
            .collect();
        shifter.process(&mut signal);
        let rms = (signal[24_000..].iter().map(|x| x * x).sum::<f32>() / 24_000.0).sqrt();
        assert!((rms - 0.212).abs() < 0.01, "rms {rms}");
    }
}
