/// Second-order Butterworth high-pass filter (biquad).
pub struct HighPassFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl HighPassFilter {
    pub fn new(cutoff_hz: f32, sample_rate: u32) -> Self {
        let omega = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate as f32;
        let cos_omega = omega.cos();
        let sin_omega = omega.sin();
        let alpha = sin_omega / (2.0 * std::f32::consts::FRAC_1_SQRT_2);

        let b0 = (1.0 + cos_omega) / 2.0;
        let b1 = -(1.0 + cos_omega);
        let b2 = (1.0 + cos_omega) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_omega;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            let x0 = *sample;
            let y0 = self.b0 * x0 + self.b1 * self.x1 + self.b2 * self.x2
                - self.a1 * self.y1
                - self.a2 * self.y2;
            self.x2 = self.x1;
            self.x1 = x0;
            self.y2 = self.y1;
            self.y1 = y0;
            *sample = y0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_sine(freq_hz: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
        let n = (sample_rate as f32 * duration_secs) as usize;
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq_hz * i as f32 / sample_rate as f32).sin())
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn attenuates_50hz_by_at_least_6db() {
        let mut signal = generate_sine(50.0, 48000, 0.5);
        let rms_before = rms(&signal);
        let mut filter = HighPassFilter::new(80.0, 48000);
        filter.process(&mut signal);
        let rms_after = rms(&signal);
        let attenuation_db = 20.0 * (rms_after / rms_before).log10();
        assert!(
            attenuation_db < -6.0,
            "Expected >6dB attenuation at 50Hz, got {:.1}dB",
            attenuation_db
        );
    }

    #[test]
    fn passes_200hz_with_minimal_loss() {
        let mut signal = generate_sine(200.0, 48000, 0.5);
        let rms_before = rms(&signal);
        let mut filter = HighPassFilter::new(80.0, 48000);
        filter.process(&mut signal);
        let rms_after = rms(&signal);
        let attenuation_db = 20.0 * (rms_after / rms_before).log10();
        assert!(
            attenuation_db > -3.0,
            "Expected <3dB loss at 200Hz, got {:.1}dB",
            attenuation_db
        );
    }
}
