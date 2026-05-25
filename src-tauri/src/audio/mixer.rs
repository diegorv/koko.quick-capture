use std::collections::VecDeque;

use super::PersistentResampler;

pub struct AudioMixerRingBuffer {
    mic_buffer: VecDeque<f32>,
    sys_buffer: VecDeque<f32>,
    window_samples: usize,
    pub(crate) max_buffer_samples: usize,
    has_system: bool,
    sys_resampler: Option<PersistentResampler>,
    pub(crate) overflow_mic: u32,
    pub(crate) overflow_sys: u32,
    pub(crate) zero_pad_count: u32,
    last_health_report: std::time::Instant,
}

impl AudioMixerRingBuffer {
    pub fn new(mic_rate: u32, sys_rate: Option<u32>, has_system: bool) -> Self {
        Self::with_bluetooth(mic_rate, sys_rate, has_system, false)
    }

    pub fn with_bluetooth(mic_rate: u32, sys_rate: Option<u32>, has_system: bool, bluetooth: bool) -> Self {
        let window_ms = if bluetooth { 800.0 } else { 600.0 };
        let window_samples = (mic_rate as f32 * window_ms / 1000.0) as usize;
        let buffer_mult = if bluetooth { 12 } else { 8 };
        let max_buffer_samples = window_samples * buffer_mult;

        let sys_resampler = match sys_rate {
            Some(sr) if sr != mic_rate && has_system => {
                match PersistentResampler::new(sr, mic_rate) {
                    Ok(r) => {
                        eprintln!(
                            "[mixer] system audio resampler: {}Hz -> {}Hz",
                            sr, mic_rate
                        );
                        Some(r)
                    }
                    Err(e) => {
                        eprintln!("[mixer] failed to create system resampler: {e}");
                        None
                    }
                }
            }
            _ => None,
        };

        Self {
            mic_buffer: VecDeque::with_capacity(window_samples * 2),
            sys_buffer: VecDeque::with_capacity(window_samples * 2),
            window_samples,
            max_buffer_samples,
            has_system,
            sys_resampler,
            overflow_mic: 0,
            overflow_sys: 0,
            zero_pad_count: 0,
            last_health_report: std::time::Instant::now(),
        }
    }

    pub fn push_mic(&mut self, samples: &[f32]) {
        self.mic_buffer.extend(samples);
        self.trim_overflow(&Source::Mic);
    }

    pub fn push_system(&mut self, samples: &[f32]) {
        if let Some(ref mut resampler) = self.sys_resampler {
            match resampler.process(samples) {
                Ok(resampled) => self.sys_buffer.extend(resampled),
                Err(e) => {
                    eprintln!("[mixer] system resample failed: {e}");
                    self.sys_buffer.extend(samples);
                }
            }
        } else {
            self.sys_buffer.extend(samples);
        }
        self.trim_overflow(&Source::System);
    }

    pub fn extract_mixed(&mut self) -> Option<Vec<f32>> {
        if !self.has_system {
            if self.mic_buffer.len() >= self.window_samples {
                let mic: Vec<f32> = self.mic_buffer.drain(..self.window_samples).collect();
                return Some(mic);
            }
            return None;
        }

        if self.mic_buffer.len() < self.window_samples
            && self.sys_buffer.len() < self.window_samples
        {
            return None;
        }

        let mic_window = self.drain_or_pad(&Source::Mic);
        let sys_window = self.drain_or_pad(&Source::System);

        let mixed: Vec<f32> = mic_window
            .iter()
            .zip(sys_window.iter())
            .map(|(&m, &s)| soft_clip(m + s))
            .collect();

        Some(mixed)
    }

    pub fn drain_remaining(&mut self) -> Vec<f32> {
        if !self.has_system {
            return self.mic_buffer.drain(..).collect();
        }

        let len = self.mic_buffer.len().max(self.sys_buffer.len());
        if len == 0 {
            return Vec::new();
        }

        let mic: Vec<f32> = self.mic_buffer.drain(..).collect();
        let sys: Vec<f32> = self.sys_buffer.drain(..).collect();

        (0..len)
            .map(|i| {
                let m = mic.get(i).copied().unwrap_or(0.0);
                let s = sys.get(i).copied().unwrap_or(0.0);
                soft_clip(m + s)
            })
            .collect()
    }

    fn drain_or_pad(&mut self, source: &Source) -> Vec<f32> {
        let buf = match source {
            Source::Mic => &mut self.mic_buffer,
            Source::System => &mut self.sys_buffer,
        };

        if buf.len() >= self.window_samples {
            buf.drain(..self.window_samples).collect()
        } else if !buf.is_empty() {
            let mut window: Vec<f32> = buf.drain(..).collect();
            window.resize(self.window_samples, 0.0);
            window
        } else {
            self.zero_pad_count += 1;
            vec![0.0; self.window_samples]
        }
    }

    pub fn maybe_report_health(&mut self) {
        if self.last_health_report.elapsed().as_secs() >= 30 {
            let mic_fill = self.mic_buffer.len();
            let sys_fill = self.sys_buffer.len();
            let any_activity = self.overflow_mic > 0 || self.overflow_sys > 0 || self.zero_pad_count > 0;
            if any_activity || mic_fill > 0 || sys_fill > 0 {
                eprintln!(
                    "[mixer] health: mic_buf={}/{} sys_buf={}/{} overflows={}mic/{}sys zero_pads={}",
                    mic_fill, self.max_buffer_samples,
                    sys_fill, self.max_buffer_samples,
                    self.overflow_mic, self.overflow_sys,
                    self.zero_pad_count
                );
            }
            self.overflow_mic = 0;
            self.overflow_sys = 0;
            self.zero_pad_count = 0;
            self.last_health_report = std::time::Instant::now();
        }
    }

    pub fn flush_resampler(&mut self) {
        if let Some(ref mut resampler) = self.sys_resampler {
            if let Ok(flushed) = resampler.flush() {
                self.sys_buffer.extend(flushed);
            }
        }
    }

    fn trim_overflow(&mut self, source: &Source) {
        let buf = match source {
            Source::Mic => &mut self.mic_buffer,
            Source::System => &mut self.sys_buffer,
        };
        if buf.len() > self.max_buffer_samples {
            let overflow = buf.len() - self.max_buffer_samples;
            eprintln!(
                "[mixer] {} buffer overflow, dropping {} samples",
                match source {
                    Source::Mic => "mic",
                    Source::System => "system",
                },
                overflow
            );
            buf.drain(..overflow);
            match source {
                Source::Mic => self.overflow_mic += 1,
                Source::System => self.overflow_sys += 1,
            }
        }
    }
}

enum Source {
    Mic,
    System,
}

fn soft_clip(sample: f32) -> f32 {
    let abs = sample.abs();
    if abs > 1.0 {
        sample / abs
    } else {
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mic_only_passthrough() {
        let mut mixer = AudioMixerRingBuffer::new(16000, None, false);
        let samples = vec![0.5f32; 9600]; // 600ms at 16kHz
        mixer.push_mic(&samples);
        let out = mixer.extract_mixed().unwrap();
        assert_eq!(out.len(), 9600);
        assert!((out[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn mic_only_returns_none_when_insufficient() {
        let mut mixer = AudioMixerRingBuffer::new(16000, None, false);
        mixer.push_mic(&vec![0.1; 100]);
        assert!(mixer.extract_mixed().is_none());
    }

    #[test]
    fn equal_signals_sum() {
        let mut mixer = AudioMixerRingBuffer::new(16000, None, true);
        let window = (16000.0 * 0.6) as usize;
        mixer.push_mic(&vec![0.3; window]);
        mixer.push_system(&vec![0.2; window]);
        let out = mixer.extract_mixed().unwrap();
        assert_eq!(out.len(), window);
        assert!((out[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn soft_clipping_prevents_overflow() {
        let mut mixer = AudioMixerRingBuffer::new(16000, None, true);
        let window = (16000.0 * 0.6) as usize;
        mixer.push_mic(&vec![0.8; window]);
        mixer.push_system(&vec![0.8; window]);
        let out = mixer.extract_mixed().unwrap();
        for &s in &out {
            assert!(s.abs() <= 1.0, "sample {} exceeds 1.0", s);
        }
    }

    #[test]
    fn zero_padding_when_system_behind() {
        let mut mixer = AudioMixerRingBuffer::new(16000, None, true);
        let window = (16000.0 * 0.6) as usize;
        mixer.push_mic(&vec![0.5; window]);
        // system has nothing -> zero-padded
        let out = mixer.extract_mixed().unwrap();
        assert_eq!(out.len(), window);
        // Should be mic signal only (system zero-padded)
        assert!((out[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn overflow_trims_oldest() {
        let mut mixer = AudioMixerRingBuffer::new(16000, None, true);
        // Push way more than max_buffer
        let huge = vec![0.1; mixer.max_buffer_samples + 5000];
        mixer.push_mic(&huge);
        assert!(mixer.mic_buffer.len() <= mixer.max_buffer_samples);
    }

    #[test]
    fn drain_remaining_mixes_uneven_buffers() {
        let mut mixer = AudioMixerRingBuffer::new(16000, None, true);
        mixer.push_mic(&vec![0.4; 1000]);
        mixer.push_system(&vec![0.3; 500]);
        let out = mixer.drain_remaining();
        assert_eq!(out.len(), 1000);
        // First 500: mixed (0.4 + 0.3 = 0.7)
        assert!((out[0] - 0.7).abs() < 1e-6);
        // Last 500: mic only (0.4 + 0.0 = 0.4)
        assert!((out[500] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn soft_clip_function() {
        assert!((soft_clip(0.5) - 0.5).abs() < 1e-6);
        assert!((soft_clip(1.5) - 1.0).abs() < 1e-6);
        assert!((soft_clip(-1.5) - (-1.0)).abs() < 1e-6);
        assert!((soft_clip(0.0)).abs() < 1e-6);
    }

    #[test]
    fn health_counters_track_overflow_and_pads() {
        let mut mixer = AudioMixerRingBuffer::new(16000, None, true);
        let huge = vec![0.1; mixer.max_buffer_samples + 5000];
        mixer.push_mic(&huge);
        assert!(mixer.overflow_mic > 0);
        assert_eq!(mixer.overflow_sys, 0);

        // Extract window with empty system -> zero pad
        let window = (16000.0 * 0.6) as usize;
        mixer.push_mic(&vec![0.5; window]);
        let _ = mixer.extract_mixed();
        assert!(mixer.zero_pad_count > 0);
    }

    #[test]
    fn multiple_windows_extracted_sequentially() {
        let mut mixer = AudioMixerRingBuffer::new(16000, None, true);
        let window = (16000.0 * 0.6) as usize;
        mixer.push_mic(&vec![0.1; window * 3]);
        mixer.push_system(&vec![0.2; window * 3]);

        let out1 = mixer.extract_mixed().unwrap();
        let out2 = mixer.extract_mixed().unwrap();
        let out3 = mixer.extract_mixed().unwrap();
        assert_eq!(out1.len(), window);
        assert_eq!(out2.len(), window);
        assert_eq!(out3.len(), window);
        assert!(mixer.extract_mixed().is_none());
    }

    #[test]
    fn cross_rate_resampling_48k_system_to_16k_mic() {
        // Mic at 16kHz, system at 48kHz - system should be resampled
        let mut mixer = AudioMixerRingBuffer::new(16000, Some(48000), true);
        assert!(mixer.sys_resampler.is_some());

        let mic_window = (16000.0 * 0.6) as usize; // 9600 samples
        let sys_1sec = 48000usize; // 1s at 48kHz -> should become ~16000 samples at 16kHz

        mixer.push_mic(&vec![0.3; mic_window]);
        mixer.push_system(&vec![0.2; sys_1sec]);

        let out = mixer.extract_mixed().unwrap();
        assert_eq!(out.len(), mic_window);
        // Mixed signal should be non-zero (both sources contribute)
        let mean: f32 = out.iter().sum::<f32>() / out.len() as f32;
        assert!(mean > 0.4, "expected mixed signal > 0.4, got {}", mean);
    }

    #[test]
    fn same_rate_skips_resampler() {
        let mixer = AudioMixerRingBuffer::new(48000, Some(48000), true);
        assert!(mixer.sys_resampler.is_none());
    }
}
