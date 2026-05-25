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

fn write_pcm_wav(path: &std::path::Path, samples: &[f32]) -> Result<()> {
    use std::io::BufWriter;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let file = std::fs::File::create(path)?;
    let mut writer = hound::WavWriter::new(BufWriter::new(file), spec)?;
    for &sample in samples {
        let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_sample(pcm)?;
    }
    writer.finalize()?;
    Ok(())
}

pub fn save_m4a(path: &std::path::Path, samples: &[f32]) -> Result<()> {
    let tmp_wav = path.with_extension("tmp.wav");
    write_pcm_wav(&tmp_wav, samples)?;

    let output = std::process::Command::new("afconvert")
        .arg(&tmp_wav)
        .arg(path)
        .args(["-d", "aac", "-f", "m4af"])
        .output()?;

    let _ = std::fs::remove_file(&tmp_wav);

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "afconvert failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_pcm_wav_creates_readable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        let samples: Vec<f32> = (0..1600)
            .map(|i| (i as f32 * 0.01).sin())
            .collect();
        write_pcm_wav(&path, &samples).unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 16000);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);
        let read_samples: Vec<i16> = reader
            .into_samples::<i16>()
            .map(|s| s.unwrap())
            .collect();
        assert_eq!(read_samples.len(), samples.len());
    }

    #[test]
    fn save_m4a_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.m4a");
        let samples: Vec<f32> = (0..16000)
            .map(|i| (i as f32 * 0.01).sin())
            .collect();
        save_m4a(&path, &samples).unwrap();
        assert!(path.exists());
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
        assert!(!path.with_extension("tmp.wav").exists());
    }
}
