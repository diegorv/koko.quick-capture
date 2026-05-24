use nnnoiseless::DenoiseState;

const RNNOISE_FRAME_SIZE: usize = DenoiseState::FRAME_SIZE;

pub struct Denoiser {
    state: Box<DenoiseState<'static>>,
}

impl Denoiser {
    pub fn new() -> Self {
        Self {
            state: DenoiseState::new(),
        }
    }

    /// Process f32 samples at 48kHz. Modifies in-place.
    /// Samples not filling a complete frame are buffered internally
    /// by processing them with zero-padding (acceptable at chunk boundaries).
    pub fn process(&mut self, samples: &mut [f32]) {
        let mut input = [0.0f32; RNNOISE_FRAME_SIZE];
        let mut output = [0.0f32; RNNOISE_FRAME_SIZE];

        let chunks = samples.chunks_mut(RNNOISE_FRAME_SIZE);
        for chunk in chunks {
            input[..chunk.len()].copy_from_slice(chunk);
            if chunk.len() < RNNOISE_FRAME_SIZE {
                input[chunk.len()..].fill(0.0);
            }
            self.state.process_frame(&mut output, &input);
            chunk.copy_from_slice(&output[..chunk.len()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn reduces_noise_on_random_input() {
        let mut rng_state: u32 = 42;
        let mut noise: Vec<f32> = (0..48000)
            .map(|_| {
                // Simple LCG PRNG for deterministic white noise
                rng_state = rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
                (rng_state as f32 / u32::MAX as f32) * 2.0 - 1.0
            })
            .collect();
        let rms_before = rms(&noise);
        let mut denoiser = Denoiser::new();
        denoiser.process(&mut noise);
        let rms_after = rms(&noise);
        assert!(
            rms_after < rms_before,
            "Expected noise reduction, before={:.4} after={:.4}",
            rms_before,
            rms_after
        );
    }
}
