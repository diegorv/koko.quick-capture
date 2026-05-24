pub mod denoise;
mod devices;
pub mod filter;
pub mod mixer;
pub mod normalize;
pub mod pool;
mod resample;
mod stream;
pub mod vad;

pub use devices::{is_likely_bluetooth, list_input_devices, AudioDevice, DeviceType, SelectedDevice};
pub use pool::BufferPool;
pub use resample::{resample_to_16khz, resample_to_48khz, PersistentResampler};
pub use stream::AudioCapture;

pub enum AudioChunk {
    Mic(Vec<f32>),
    System(Vec<f32>),
}

use anyhow::Result;

pub fn save_wav(path: &std::path::Path, samples: &[f32]) -> Result<()> {
    use std::io::BufWriter;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let file = std::fs::File::create(path)?;
    let buf_writer = BufWriter::new(file);
    let mut writer = hound::WavWriter::new(buf_writer, spec)?;
    for &sample in samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_wav_creates_readable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        let samples: Vec<f32> = (0..1600)
            .map(|i| (i as f32 * 0.01).sin())
            .collect();
        save_wav(&path, &samples).unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 16000);
        assert_eq!(spec.bits_per_sample, 32);
        let read_samples: Vec<f32> = reader
            .into_samples::<f32>()
            .map(|s| s.unwrap())
            .collect();
        assert_eq!(read_samples.len(), samples.len());
        for (a, b) in read_samples.iter().zip(samples.iter()) {
            assert!((a - b).abs() < 1e-6, "sample mismatch: {} vs {}", a, b);
        }
    }

    #[test]
    fn save_wav_empty_samples() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.wav");
        save_wav(&path, &[]).unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        let count = reader.into_samples::<f32>().count();
        assert_eq!(count, 0);
    }
}
