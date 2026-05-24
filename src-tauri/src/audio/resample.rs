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

const CHUNK_SIZE: usize = 4096;

pub struct PersistentResampler {
    resampler: Async<f32>,
    from_rate: u32,
    to_rate: u32,
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
        })
    }

    pub fn process(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        if self.from_rate == self.to_rate {
            return Ok(samples.to_vec());
        }

        use audioadapter_buffers::direct::SequentialSliceOfVecs;

        let mut output = Vec::new();
        for chunk in samples.chunks(CHUNK_SIZE) {
            let padded;
            let input = if chunk.len() < CHUNK_SIZE {
                padded = {
                    let mut v = chunk.to_vec();
                    v.resize(CHUNK_SIZE, 0.0);
                    v
                };
                &padded
            } else {
                chunk
            };

            let data = vec![input.to_vec()];
            let waves_in = SequentialSliceOfVecs::new(&data[..], 1, CHUNK_SIZE)
                .map_err(|e| anyhow::anyhow!("Buffer error: {}", e))?;
            let waves_out = self.resampler.process(&waves_in, 0, None)?;
            let out_data = waves_out.take_data();

            if chunk.len() < CHUNK_SIZE {
                let expected = (chunk.len() as f64 * self.to_rate as f64 / self.from_rate as f64) as usize;
                output.extend_from_slice(&out_data[..expected.min(out_data.len())]);
            } else {
                output.extend_from_slice(&out_data);
            }
        }
        Ok(output)
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
}
