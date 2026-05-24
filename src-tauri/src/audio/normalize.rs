use ebur128::{EbuR128, Mode};

const TARGET_LUFS: f64 = -23.0;
const TRUE_PEAK_LIMIT: f32 = 0.891; // -1 dBTP
const MAX_GAIN: f32 = 32.0;

pub struct LoudnessNormalizer {
    meter: EbuR128,
    sample_rate: u32,
}

impl LoudnessNormalizer {
    pub fn new(sample_rate: u32) -> Self {
        let meter = EbuR128::new(1, sample_rate, Mode::I | Mode::TRUE_PEAK)
            .expect("failed to create EBU R128 meter");
        Self { meter, sample_rate }
    }

    /// Measure loudness and apply gain normalization in-place.
    /// Processes in 512-sample blocks for responsive gain updates.
    pub fn process(&mut self, samples: &mut [f32]) {
        if samples.is_empty() {
            return;
        }

        // Feed all samples to the meter for loudness measurement
        for chunk in samples.chunks(512) {
            let _ = self.meter.add_frames_f32(chunk);
        }

        let loudness = self.meter.loudness_global().unwrap_or(TARGET_LUFS);
        if !loudness.is_finite() || loudness < -70.0 {
            return;
        }

        let gain_db = TARGET_LUFS - loudness;
        let gain = 10.0f32.powf(gain_db as f32 / 20.0).min(MAX_GAIN);

        if (gain - 1.0).abs() < 0.01 {
            return;
        }

        for sample in samples.iter_mut() {
            *sample *= gain;
            if *sample > TRUE_PEAK_LIMIT {
                *sample = TRUE_PEAK_LIMIT;
            } else if *sample < -TRUE_PEAK_LIMIT {
                *sample = -TRUE_PEAK_LIMIT;
            }
        }
    }

    #[allow(dead_code)]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn quiet_signal_gets_amplified() {
        let mut signal: Vec<f32> = (0..48000)
            .map(|i| 0.001 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48000.0).sin())
            .collect();
        let rms_before = rms(&signal);
        let mut norm = LoudnessNormalizer::new(48000);
        norm.process(&mut signal);
        let rms_after = rms(&signal);
        assert!(
            rms_after > rms_before,
            "Expected amplification, before={:.6} after={:.6}",
            rms_before,
            rms_after
        );
    }

    #[test]
    fn loud_signal_gets_limited() {
        let mut signal: Vec<f32> = (0..48000)
            .map(|i| 0.95 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48000.0).sin())
            .collect();
        let mut norm = LoudnessNormalizer::new(48000);
        norm.process(&mut signal);
        let peak = signal.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        assert!(
            peak <= TRUE_PEAK_LIMIT + 0.001,
            "Peak {:.4} exceeds limit {:.4}",
            peak,
            TRUE_PEAK_LIMIT
        );
    }
}
