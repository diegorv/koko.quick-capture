use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::devices::{find_device, SelectedDevice};
use super::resample::audio_to_mono;

pub struct AudioCapture {
    pub sample_rate: u32,
}

impl AudioCapture {
    pub fn start(
        sample_sender: mpsc::UnboundedSender<Vec<f32>>,
        is_recording: Arc<AtomicBool>,
        selected: Option<SelectedDevice>,
        peak_level: Arc<AtomicU32>,
    ) -> Result<(cpal::Stream, Self)> {
        let device = if let Some(ref sel) = selected {
            let (dev, _is_sck) = find_device(&sel.name, &sel.device_type)?;
            dev
        } else {
            let host = cpal::default_host();
            host.default_input_device()
                .ok_or_else(|| anyhow!("No input device available"))?
        };

        eprintln!(
            "[audio] Using device: {}",
            device.name().unwrap_or_default()
        );

        let config = device.default_input_config()?;
        let channels = config.channels();
        let sample_rate = config.sample_rate().0;

        eprintln!(
            "[audio] Config: {} Hz, {} channels",
            sample_rate, channels
        );

        let stream = device.build_input_stream(
            &config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mono = audio_to_mono(data, channels);
                let peak = mono.iter().fold(0.0f32, |max, &s| max.max(s.abs()));
                peak_level.fetch_max(peak.to_bits(), Ordering::Relaxed);

                if !is_recording.load(Ordering::Relaxed) {
                    return;
                }
                let _ = sample_sender.send(mono);
            },
            |err| {
                log::error!("Audio stream error: {}", err);
            },
            None,
            None,
        )?;

        stream.play()?;

        Ok((stream, AudioCapture { sample_rate }))
    }
}
