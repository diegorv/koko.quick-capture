use anyhow::{anyhow, Result};
use silero::{VadConfig, VadSession, VadTransition};
use std::collections::VecDeque;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SpeechSegment {
    pub samples: Vec<f32>,
    pub start_timestamp_ms: f64,
    pub end_timestamp_ms: f64,
}

const VAD_SAMPLE_RATE: u32 = 16000;

pub struct ContinuousVadProcessor {
    session: VadSession,
    chunk_size: usize,
    sample_rate: u32,
    buffer: Vec<f32>,
    speech_segments: VecDeque<SpeechSegment>,
    current_speech: Vec<f32>,
    in_speech: bool,
    processed_samples: usize,
    speech_start_sample: usize,
}

impl ContinuousVadProcessor {
    pub fn new(input_sample_rate: u32, redemption_time_ms: u32) -> Result<Self> {
        let mut config = VadConfig::default();
        config.sample_rate = VAD_SAMPLE_RATE as usize;
        config.positive_speech_threshold = 0.50;
        config.negative_speech_threshold = 0.35;
        config.redemption_time = Duration::from_millis(redemption_time_ms as u64);
        config.pre_speech_pad = Duration::from_millis(300);
        config.post_speech_pad = Duration::from_millis(400);
        config.min_speech_time = Duration::from_millis(250);

        let session = VadSession::new(config)
            .map_err(|e| anyhow!("Failed to create VAD session: {:?}", e))?;

        let chunk_size = (VAD_SAMPLE_RATE as f32 * 0.03) as usize; // 480 samples = 30ms

        eprintln!(
            "[vad] processor created: input={}Hz, vad={}Hz, chunk={} samples, redemption={}ms",
            input_sample_rate, VAD_SAMPLE_RATE, chunk_size, redemption_time_ms
        );

        Ok(Self {
            session,
            chunk_size,
            sample_rate: input_sample_rate,
            buffer: Vec::with_capacity(chunk_size * 2),
            speech_segments: VecDeque::new(),
            current_speech: Vec::new(),
            in_speech: false,
            processed_samples: 0,
            speech_start_sample: 0,
        })
    }

    pub fn process_audio(&mut self, samples: &[f32]) -> Result<Vec<SpeechSegment>> {
        let resampled = if self.sample_rate == VAD_SAMPLE_RATE {
            samples.to_vec()
        } else {
            resample_to_16k(samples, self.sample_rate)
        };

        self.buffer.extend_from_slice(&resampled);
        let mut completed = Vec::new();

        while self.buffer.len() >= self.chunk_size {
            let chunk: Vec<f32> = self.buffer.drain(..self.chunk_size).collect();
            self.process_chunk(&chunk)?;

            while let Some(segment) = self.speech_segments.pop_front() {
                completed.push(segment);
            }
        }

        Ok(completed)
    }

    pub fn flush(&mut self) -> Result<Vec<SpeechSegment>> {
        let mut completed = Vec::new();

        if !self.buffer.is_empty() {
            let remaining = self.buffer.clone();
            self.buffer.clear();
            let mut padded = remaining;
            if padded.len() < self.chunk_size {
                padded.resize(self.chunk_size, 0.0);
            }
            self.process_chunk(&padded)?;
        }

        if self.in_speech && !self.current_speech.is_empty() {
            let start_ms = (self.speech_start_sample as f64 / VAD_SAMPLE_RATE as f64) * 1000.0;
            let end_ms = (self.processed_samples as f64 / VAD_SAMPLE_RATE as f64) * 1000.0;

            eprintln!(
                "[vad] flush: force-ending speech {:.0}ms-{:.0}ms ({} samples)",
                start_ms, end_ms, self.current_speech.len()
            );

            self.speech_segments.push_back(SpeechSegment {
                samples: self.current_speech.clone(),
                start_timestamp_ms: start_ms,
                end_timestamp_ms: end_ms,
            });
            self.current_speech.clear();
            self.in_speech = false;
        }

        while let Some(segment) = self.speech_segments.pop_front() {
            completed.push(segment);
        }

        Ok(completed)
    }

    fn process_chunk(&mut self, chunk: &[f32]) -> Result<()> {
        if self.current_speech.len() > 1_000_000 {
            eprintln!(
                "[vad] large speech buffer: {} samples ({:.1}s)",
                self.current_speech.len(),
                self.current_speech.len() as f64 / VAD_SAMPLE_RATE as f64
            );
        }

        let transitions = self
            .session
            .process(chunk)
            .map_err(|e| anyhow!("VAD processing failed: {}", e))?;

        for transition in transitions {
            match transition {
                VadTransition::SpeechStart { timestamp_ms } => {
                    self.in_speech = true;
                    self.speech_start_sample =
                        self.processed_samples + (timestamp_ms * VAD_SAMPLE_RATE as usize / 1000);
                    self.current_speech.clear();
                }
                VadTransition::SpeechEnd {
                    start_timestamp_ms,
                    end_timestamp_ms,
                    samples,
                } => {
                    self.in_speech = false;

                    let speech_samples = if !samples.is_empty() {
                        samples
                    } else {
                        self.current_speech.clone()
                    };

                    if !speech_samples.is_empty() {
                        eprintln!(
                            "[vad] speech segment: {:.0}ms-{:.0}ms ({} samples)",
                            start_timestamp_ms, end_timestamp_ms, speech_samples.len()
                        );
                        self.speech_segments.push_back(SpeechSegment {
                            samples: speech_samples,
                            start_timestamp_ms: start_timestamp_ms as f64,
                            end_timestamp_ms: end_timestamp_ms as f64,
                        });
                    }

                    self.current_speech.clear();
                }
            }
        }

        if self.in_speech {
            self.current_speech.extend_from_slice(chunk);
        }

        self.processed_samples += chunk.len();
        Ok(())
    }
}

fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    let ratio = from_rate as f64 / VAD_SAMPLE_RATE as f64;
    let output_len = (samples.len() as f64 / ratio) as usize;
    let mut resampled = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos as usize;
        let fraction = (src_pos - src_idx as f64) as f32;

        if src_idx + 1 < samples.len() {
            let s1 = samples[src_idx];
            let s2 = samples[src_idx + 1];
            resampled.push(s1 + (s2 - s1) * fraction);
        } else if src_idx < samples.len() {
            resampled.push(samples[src_idx]);
        }
    }

    resampled
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_speech_signal(duration_secs: f32) -> Vec<f32> {
        let samples = (duration_secs * VAD_SAMPLE_RATE as f32) as usize;
        (0..samples)
            .map(|i| {
                let t = i as f32 / VAD_SAMPLE_RATE as f32;
                let freq = 200.0 + (t * 50.0).sin() * 100.0;
                let amp = 0.3 + 0.1 * (t * 5.0).sin();
                amp * (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect()
    }

    fn generate_silence(duration_secs: f32) -> Vec<f32> {
        vec![0.0f32; (duration_secs * VAD_SAMPLE_RATE as f32) as usize]
    }

    fn generate_speech_silence_speech(
        speech1_secs: f32,
        silence_secs: f32,
        speech2_secs: f32,
    ) -> Vec<f32> {
        let mut audio = generate_speech_signal(speech1_secs);
        audio.extend(generate_silence(silence_secs));
        audio.extend(generate_speech_signal(speech2_secs));
        audio
    }

    #[test]
    fn silence_produces_no_segments() {
        let mut vad = ContinuousVadProcessor::new(16000, 400).unwrap();
        let silence = generate_silence(5.0);
        let segments = vad.process_audio(&silence).unwrap();
        let flushed = vad.flush().unwrap();
        assert!(
            segments.is_empty() && flushed.is_empty(),
            "silence should produce no segments"
        );
    }

    #[test]
    fn speech_produces_segment() {
        let mut vad = ContinuousVadProcessor::new(16000, 400).unwrap();
        let mut audio = generate_speech_signal(3.0);
        audio.extend(generate_silence(1.0));

        let mut all = vad.process_audio(&audio).unwrap();
        all.extend(vad.flush().unwrap());

        assert!(
            !all.is_empty(),
            "speech followed by silence should produce at least one segment"
        );
        for seg in &all {
            assert!(
                seg.samples.len() >= 400, // > 25ms
                "segment too short: {} samples",
                seg.samples.len()
            );
        }
    }

    #[test]
    fn flush_captures_in_progress_speech() {
        let mut vad = ContinuousVadProcessor::new(16000, 400).unwrap();
        // Use longer speech to ensure VAD detects it
        let speech = generate_speech_signal(5.0);

        let segments = vad.process_audio(&speech).unwrap();
        let flushed = vad.flush().unwrap();
        let total: Vec<_> = segments.into_iter().chain(flushed).collect();

        assert!(
            !total.is_empty(),
            "flush should emit in-progress speech"
        );
    }

    #[test]
    fn speech_silence_speech_produces_two_segments() {
        let mut vad = ContinuousVadProcessor::new(16000, 400).unwrap();
        let audio = generate_speech_silence_speech(3.0, 2.0, 3.0);

        let mut all = vad.process_audio(&audio).unwrap();
        all.extend(vad.flush().unwrap());

        assert!(
            all.len() >= 2,
            "speech-silence-speech should produce >= 2 segments, got {}",
            all.len()
        );
    }

    #[test]
    fn chunked_processing_matches_single_pass() {
        let audio = generate_speech_silence_speech(3.0, 2.0, 3.0);

        // Single pass
        let mut vad1 = ContinuousVadProcessor::new(16000, 400).unwrap();
        let mut segs1 = vad1.process_audio(&audio).unwrap();
        segs1.extend(vad1.flush().unwrap());

        // Chunked (1000 samples at a time)
        let mut vad2 = ContinuousVadProcessor::new(16000, 400).unwrap();
        let mut segs2 = Vec::new();
        for chunk in audio.chunks(1000) {
            segs2.extend(vad2.process_audio(chunk).unwrap());
        }
        segs2.extend(vad2.flush().unwrap());

        let diff = (segs1.len() as i32 - segs2.len() as i32).abs();
        assert!(
            diff <= 1,
            "chunked vs single: {} vs {} segments",
            segs2.len(),
            segs1.len()
        );
    }

    #[test]
    fn timestamps_are_monotonic() {
        let mut vad = ContinuousVadProcessor::new(16000, 400).unwrap();
        let audio = generate_speech_silence_speech(2.0, 1.5, 2.0);
        let mut segments = vad.process_audio(&audio).unwrap();
        segments.extend(vad.flush().unwrap());

        for seg in &segments {
            assert!(
                seg.end_timestamp_ms >= seg.start_timestamp_ms,
                "end ({}) < start ({})",
                seg.end_timestamp_ms,
                seg.start_timestamp_ms
            );
        }
        for w in segments.windows(2) {
            assert!(
                w[1].start_timestamp_ms >= w[0].end_timestamp_ms,
                "segments overlap: [0] ends {}, [1] starts {}",
                w[0].end_timestamp_ms,
                w[1].start_timestamp_ms
            );
        }
    }

    #[test]
    fn resampling_from_48k() {
        let mut vad = ContinuousVadProcessor::new(48000, 400).unwrap();
        // Generate speech-like signal at 48kHz (multi-frequency, amplitude-modulated)
        let duration_secs = 3.0f32;
        let rate = 48000u32;
        let samples = (duration_secs * rate as f32) as usize;
        let audio: Vec<f32> = (0..samples)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let freq = 200.0 + (t * 50.0).sin() * 100.0;
                let amp = 0.3 + 0.1 * (t * 5.0).sin();
                amp * (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect();
        let silence = vec![0.0f32; rate as usize]; // 1s silence
        let mut full = audio;
        full.extend(silence);

        let mut segs = vad.process_audio(&full).unwrap();
        segs.extend(vad.flush().unwrap());
        assert!(
            !segs.is_empty(),
            "48kHz input should produce segments after resampling"
        );
    }

    #[test]
    fn resample_to_16k_preserves_length_ratio() {
        let from_rate = 48000u32;
        let input = vec![0.5f32; from_rate as usize]; // 1 second at 48kHz
        let output = resample_to_16k(&input, from_rate);
        let expected = VAD_SAMPLE_RATE as usize; // 16000 samples
        let tolerance = 2;
        assert!(
            (output.len() as i32 - expected as i32).unsigned_abs() <= tolerance,
            "expected ~{} samples, got {}",
            expected,
            output.len()
        );
    }
}
