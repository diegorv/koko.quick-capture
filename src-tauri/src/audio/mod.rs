mod devices;
pub mod filter;
mod resample;
mod stream;

pub use devices::{list_input_devices, AudioDevice, DeviceType, SelectedDevice};
pub use resample::resample_to_16khz;
pub use stream::AudioCapture;

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
