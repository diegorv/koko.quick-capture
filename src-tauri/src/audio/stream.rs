use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::devices::{find_device, SelectedDevice};
use super::resample::audio_to_mono;
use super::AudioChunk;

pub struct AudioCapture {
    pub sample_rate: u32,
}

impl AudioCapture {
    pub fn start(
        sample_sender: mpsc::UnboundedSender<AudioChunk>,
        is_recording: Arc<AtomicBool>,
        selected: Option<SelectedDevice>,
        peak_level: Arc<AtomicU32>,
        is_system: bool,
    ) -> Result<(cpal::Stream, Self)> {
        let device = if let Some(ref sel) = selected {
            match find_device(&sel.name, &sel.device_type) {
                Ok((dev, _)) => dev,
                Err(e) => {
                    eprintln!("[audio] Saved device '{}' not found ({}), falling back to default", sel.name, e);
                    let host = cpal::default_host();
                    host.default_input_device()
                        .ok_or_else(|| anyhow!("No input device available"))?
                }
            }
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
                let prev = peak_level.load(Ordering::Relaxed);
                let prev_f = f32::from_bits(prev);
                if peak > prev_f {
                    peak_level.store(peak.to_bits(), Ordering::Relaxed);
                }

                if !is_recording.load(Ordering::Relaxed) {
                    return;
                }
                let chunk = if is_system {
                    AudioChunk::System(mono)
                } else {
                    AudioChunk::Mic(mono)
                };
                let _ = sample_sender.send(chunk);
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
