use anyhow::Result;
use rubato::{
    Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};

const SINC_PARAMS: SincInterpolationParameters = SincInterpolationParameters {
    sinc_len: 256,
    f_cutoff: 0.95,
    interpolation: SincInterpolationType::Linear,
    oversampling_factor: 256,
    window: WindowFunction::BlackmanHarris2,
};

const CHUNK_SIZE: usize = 512;

pub struct PersistentResampler {
    resampler: Async<f32>,
    from_rate: u32,
    to_rate: u32,
    buffer: Vec<f32>,
}

impl PersistentResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Result<Self> {
        let resampler = Async::<f32>::new_sinc(
            to_rate as f64 / from_rate as f64,
            2.0,
            &SINC_PARAMS,
            CHUNK_SIZE,
            1,
            FixedAsync::Input,
        )?;
        Ok(Self {
            resampler,
            from_rate,
            to_rate,
            buffer: Vec::with_capacity(CHUNK_SIZE * 2),
        })
    }

    pub fn process(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        if self.from_rate == self.to_rate {
            return Ok(samples.to_vec());
        }

        use audioadapter_buffers::direct::SequentialSliceOfVecs;

        self.buffer.extend_from_slice(samples);

        let mut output = Vec::new();
        while self.buffer.len() >= CHUNK_SIZE {
            let chunk: Vec<f32> = self.buffer.drain(..CHUNK_SIZE).collect();
            let data = vec![chunk];
            let waves_in = SequentialSliceOfVecs::new(&data[..], 1, CHUNK_SIZE)
                .map_err(|e| anyhow::anyhow!("Buffer error: {}", e))?;
            let waves_out = self.resampler.process(&waves_in, 0, None)?;
            output.extend_from_slice(&waves_out.take_data());
        }
        Ok(output)
    }

    pub fn flush(&mut self) -> Result<Vec<f32>> {
        if self.from_rate == self.to_rate {
            return Ok(self.buffer.drain(..).collect());
        }
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }

        use audioadapter_buffers::direct::SequentialSliceOfVecs;

        let real_len = self.buffer.len();
        self.buffer.resize(CHUNK_SIZE, 0.0);
        let data = vec![self.buffer.drain(..).collect::<Vec<f32>>()];
        let waves_in = SequentialSliceOfVecs::new(&data[..], 1, CHUNK_SIZE)
            .map_err(|e| anyhow::anyhow!("Buffer error: {}", e))?;
        let waves_out = self.resampler.process(&waves_in, 0, None)?;
        let out = waves_out.take_data();
        let expected = (real_len as f64 * self.to_rate as f64 / self.from_rate as f64) as usize;
        Ok(out[..expected.min(out.len())].to_vec())
    }
}

pub(super) fn audio_to_mono(data: &[f32], channels: u16) -> Vec<f32> {
    if channels == 1 {
        return data.to_vec();
    }
    data.chunks(channels as usize)
        .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
        .collect()
}

pub fn resample_to_16khz(samples: &[f32], from_rate: u32) -> Result<Vec<f32>> {
    resample(samples, from_rate, 16000)
}

pub fn resample_to_48khz(samples: &[f32], from_rate: u32) -> Result<Vec<f32>> {
    resample(samples, from_rate, 48000)
}

fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>> {
    if from_rate == to_rate {
        return Ok(samples.to_vec());
    }

    let mut resampler = Async::<f32>::new_sinc(
        to_rate as f64 / from_rate as f64,
        2.0,
        &SINC_PARAMS,
        samples.len(),
        1,
        FixedAsync::Input,
    )?;

    use audioadapter_buffers::direct::SequentialSliceOfVecs;

    let waves_in_data = vec![samples.to_vec()];
    let waves_in = SequentialSliceOfVecs::new(&waves_in_data[..], 1, samples.len())
        .map_err(|e| anyhow::anyhow!("Failed to create audio buffer: {}", e))?;
    let waves_out = resampler.process(&waves_in, 0, None)?;
    Ok(waves_out.take_data())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_to_mono_mono_input_is_returned_as_clone() {
        let input = vec![0.1, 0.2, 0.3];
        let out = audio_to_mono(&input, 1);
        assert_eq!(out, input);
    }

    #[test]
    fn audio_to_mono_stereo_averages_each_pair() {
        let input = vec![1.0, -1.0, 2.0, 0.0, 4.0, 2.0];
        let out = audio_to_mono(&input, 2);
        assert_eq!(out, vec![0.0, 1.0, 3.0]);
    }

    #[test]
    fn resample_to_16khz_is_a_clone_when_already_16khz() {
        let input = vec![0.1f32, 0.2, 0.3, 0.4];
        let out = resample_to_16khz(&input, 16000).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn resample_to_16khz_48k_to_16k_produces_roughly_one_third_samples() {
        let input: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.01).sin()).collect();
        let out = resample_to_16khz(&input, 48000).unwrap();
        let expected = input.len() / 3;
        let low = (expected as f32 * 0.95) as usize;
        let high = (expected as f32 * 1.05) as usize;
        assert!(
            out.len() >= low && out.len() <= high,
            "expected output length in [{}, {}], got {}",
            low,
            high,
            out.len()
        );
    }

    // --- resample_to_48khz ---

    #[test]
    fn resample_to_48khz_is_clone_when_already_48khz() {
        let input = vec![0.1f32, 0.2, 0.3, 0.4];
        let out = resample_to_48khz(&input, 48000).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn resample_to_48khz_16k_to_48k_produces_roughly_three_times_samples() {
        let input: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.01).sin()).collect();
        let out = resample_to_48khz(&input, 16000).unwrap();
        let expected = input.len() * 3;
        let low = (expected as f32 * 0.95) as usize;
        let high = (expected as f32 * 1.05) as usize;
        assert!(
            out.len() >= low && out.len() <= high,
            "expected ~{}, got {}",
            expected,
            out.len()
        );
    }

    // --- audio_to_mono edge cases ---

    #[test]
    fn audio_to_mono_three_channels() {
        let input = vec![3.0, 6.0, 9.0, 1.0, 2.0, 3.0];
        let out = audio_to_mono(&input, 3);
        assert_eq!(out.len(), 2);
        assert!((out[0] - 6.0).abs() < 1e-6);
        assert!((out[1] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn audio_to_mono_empty_input() {
        let out = audio_to_mono(&[], 2);
        assert!(out.is_empty());
    }

    // --- PersistentResampler ---

    #[test]
    fn persistent_resampler_same_rate_passthrough() {
        let mut r = PersistentResampler::new(48000, 48000).unwrap();
        let input = vec![0.5f32; 100];
        let out = r.process(&input).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn persistent_resampler_48k_to_16k() {
        let mut r = PersistentResampler::new(48000, 16000).unwrap();
        let input: Vec<f32> = (0..CHUNK_SIZE)
            .map(|i| (i as f32 * 0.01).sin())
            .collect();
        let out = r.process(&input).unwrap();
        let expected = CHUNK_SIZE / 3;
        let tolerance = (expected as f32 * 0.1) as usize;
        assert!(
            (out.len() as i64 - expected as i64).unsigned_abs() <= tolerance as u64,
            "expected ~{}, got {}",
            expected,
            out.len()
        );
    }

    #[test]
    fn persistent_resampler_multiple_chunks_produce_output() {
        let mut r = PersistentResampler::new(48000, 16000).unwrap();
        let chunk: Vec<f32> = (0..CHUNK_SIZE)
            .map(|i| (i as f32 * 0.01).sin())
            .collect();
        let out1 = r.process(&chunk).unwrap();
        let out2 = r.process(&chunk).unwrap();
        assert!(!out1.is_empty());
        assert!(!out2.is_empty());
    }

    #[test]
    fn persistent_resampler_buffers_small_chunks() {
        let mut r = PersistentResampler::new(48000, 16000).unwrap();
        // 256 samples < CHUNK_SIZE(512) -> should buffer, return empty
        let small = vec![0.5f32; 256];
        let out = r.process(&small).unwrap();
        assert!(out.is_empty(), "sub-chunk input should buffer, got {} samples", out.len());

        // Another 256 -> total 512 = CHUNK_SIZE -> should produce output
        let out = r.process(&small).unwrap();
        assert!(!out.is_empty(), "full chunk should produce output");
    }

    #[test]
    fn persistent_resampler_flush_drains_buffered() {
        let mut r = PersistentResampler::new(48000, 16000).unwrap();
        let small = vec![0.5f32; 300];
        let out = r.process(&small).unwrap();
        assert!(out.is_empty());

        let flushed = r.flush().unwrap();
        assert!(!flushed.is_empty(), "flush should produce output from buffered samples");
        let expected = (300.0 * 16000.0 / 48000.0) as usize;
        let tolerance = (expected as f32 * 0.15) as usize;
        assert!(
            (flushed.len() as i64 - expected as i64).unsigned_abs() <= tolerance as u64,
            "expected ~{}, got {}", expected, flushed.len()
        );
    }

    #[test]
    fn persistent_resampler_small_chunks_preserve_amplitude() {
        let freq = 440.0;
        let duration = CHUNK_SIZE * 4;
        let signal: Vec<f32> = (0..duration)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * freq * i as f32 / 48000.0).sin())
            .collect();

        // Large chunks (ideal)
        let mut r_large = PersistentResampler::new(48000, 16000).unwrap();
        let out_large = r_large.process(&signal).unwrap();
        let rms_large = (out_large.iter().map(|s| s * s).sum::<f32>() / out_large.len() as f32).sqrt();

        // Small 512-sample chunks (real mic callbacks)
        let mut r_small = PersistentResampler::new(48000, 16000).unwrap();
        let mut out_small = Vec::new();
        for chunk in signal.chunks(512) {
            out_small.extend(r_small.process(chunk).unwrap());
        }
        out_small.extend(r_small.flush().unwrap());
        let rms_small = (out_small.iter().map(|s| s * s).sum::<f32>() / out_small.len() as f32).sqrt();

        assert!(
            rms_small > rms_large * 0.95,
            "buffered resampling should preserve >95% amplitude, got {:.1}%",
            (rms_small / rms_large) * 100.0
        );
    }
}
