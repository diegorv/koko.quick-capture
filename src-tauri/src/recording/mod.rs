use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use tokio::sync::mpsc;
use whisper_rs::WhisperContext;

use crate::audio::denoise::Denoiser;
use crate::audio::filter::HighPassFilter;
use crate::audio::mixer::AudioMixerRingBuffer;
use crate::audio::normalize::LoudnessNormalizer;
use crate::audio::vad::ContinuousVadProcessor;
use crate::audio::{
    is_likely_bluetooth, resample_to_16khz, resample_to_48khz, save_wav, AudioCapture,
    AudioChunk, PersistentResampler, SelectedDevice,
};
use crate::store::{CaptureInput, Store};
use crate::transcription;

const MODEL_FILENAME: &str = "ggml-large-v3-turbo-q5_0.bin";
const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin";

const VAD_MODEL_FILENAME: &str = "ggml-silero-v6.2.0.bin";
const VAD_MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-silero-v6.2.0.bin";

const VAD_REDEMPTION_TIME_MS: u32 = 400;
const FALLBACK_CHUNK_SECS: u64 = 20;
const MAX_ERRORS: u32 = 15;

struct ChunkerStats {
    segments_transcribed: u32,
    segments_skipped_short: u32,
    segments_skipped_silence: u32,
    last_report: std::time::Instant,
}

impl ChunkerStats {
    fn new() -> Self {
        Self {
            segments_transcribed: 0,
            segments_skipped_short: 0,
            segments_skipped_silence: 0,
            last_report: std::time::Instant::now(),
        }
    }

    fn maybe_report(&mut self) {
        if self.last_report.elapsed().as_secs() >= 60 {
            let total = self.segments_transcribed + self.segments_skipped_short + self.segments_skipped_silence;
            if total > 0 {
                eprintln!(
                    "[recording] stats: {} transcribed, {} skipped (short), {} skipped (silence) in last 60s",
                    self.segments_transcribed, self.segments_skipped_short, self.segments_skipped_silence
                );
            }
            self.segments_transcribed = 0;
            self.segments_skipped_short = 0;
            self.segments_skipped_silence = 0;
            self.last_report = std::time::Instant::now();
        }
    }
}

pub fn models_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("com.koko.quick-capture").join("models")
}

pub fn model_path() -> PathBuf {
    models_dir().join(MODEL_FILENAME)
}

pub fn vad_model_path() -> PathBuf {
    models_dir().join(VAD_MODEL_FILENAME)
}

fn resolve_vad_path() -> Option<String> {
    let p = vad_model_path();
    if p.exists() && validate_model_file(&p, VAD_MIN_SIZE) {
        p.to_str().map(|s| s.to_string())
    } else {
        None
    }
}

const WHISPER_MIN_SIZE: u64 = 500_000_000;
const VAD_MIN_SIZE: u64 = 800_000;
const GGML_MAGIC: [u8; 4] = [0x67, 0x67, 0x6d, 0x6c];
const GGUF_MAGIC: [u8; 4] = [0x47, 0x47, 0x55, 0x46];

fn validate_model_file(path: &std::path::Path, min_size: u64) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if meta.len() < min_size {
        eprintln!(
            "[recording] model {} too small: {} bytes (min {})",
            path.display(),
            meta.len(),
            min_size
        );
        return false;
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    use std::io::Read;
    let mut magic = [0u8; 4];
    if f.read_exact(&mut magic).is_err() {
        return false;
    }
    if magic != GGML_MAGIC && magic != GGUF_MAGIC {
        eprintln!(
            "[recording] model {} has invalid magic: {:02x?}",
            path.display(),
            magic
        );
        return false;
    }
    true
}

pub fn is_model_downloaded() -> bool {
    let path = model_path();
    path.exists() && validate_model_file(&path, WHISPER_MIN_SIZE)
}

pub fn audio_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("com.koko.quick-capture").join("audio")
}

pub async fn download_model(
    on_progress: impl Fn(u64, u64),
) -> Result<PathBuf> {
    let dir = models_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(MODEL_FILENAME);

    if path.exists() && validate_model_file(&path, WHISPER_MIN_SIZE) {
        return Ok(path);
    }
    if path.exists() {
        eprintln!("[recording] corrupt model detected, re-downloading");
        let _ = std::fs::remove_file(&path);
    }

    let tmp_path = dir.join(format!("{MODEL_FILENAME}.tmp"));

    let resp = reqwest::get(MODEL_URL).await?; // privacy-ok: downloads Whisper model from HuggingFace
    let total = resp.content_length().unwrap_or(0);

    use futures::StreamExt;
    use std::io::Write;

    let mut file = std::fs::File::create(&tmp_path)?;
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }
    file.flush()?;
    drop(file);

    std::fs::rename(&tmp_path, &path)?;

    // Also download VAD model (864KB, negligible)
    let vad_path = dir.join(VAD_MODEL_FILENAME);
    if !vad_path.exists() {
        let vad_tmp = dir.join(format!("{VAD_MODEL_FILENAME}.tmp"));
        let vad_resp = reqwest::get(VAD_MODEL_URL).await?; // privacy-ok: downloads Silero VAD model from HuggingFace
        let mut vad_file = std::fs::File::create(&vad_tmp)?;
        let mut vad_stream = vad_resp.bytes_stream();
        while let Some(chunk) = vad_stream.next().await {
            let chunk = chunk?;
            vad_file.write_all(&chunk)?;
        }
        vad_file.flush()?;
        drop(vad_file);
        std::fs::rename(&vad_tmp, &vad_path)?;
        eprintln!("[recording] VAD model downloaded");
    }

    Ok(path)
}

/// Accumulated transcript chunks from the background chunker thread.
pub struct ChunkedTranscript {
    texts: Vec<String>,
    chunks_processed: u32,
    chunks_failed: u32,
}

impl ChunkedTranscript {
    fn new() -> Self {
        Self {
            texts: Vec::new(),
            chunks_processed: 0,
            chunks_failed: 0,
        }
    }

    fn push(&mut self, text: String) {
        self.chunks_processed += 1;
        if text.is_empty() {
            return;
        }

        if let Some(prev) = self.texts.last() {
            if let Some((_prev_idx, cur_idx)) = longest_common_word_overlap(prev, &text) {
                let cur_words: Vec<&str> = text.split_whitespace().collect();
                let deduped = cur_words[cur_idx..].join(" ");
                if !deduped.is_empty() {
                    self.texts.push(deduped);
                }
                return;
            }
        }

        self.texts.push(text);
    }

    fn record_failure(&mut self) {
        self.chunks_failed += 1;
    }

    fn merged(&self) -> String {
        self.texts.join(" ").trim().to_string()
    }

    fn last_chunk(&self) -> Option<String> {
        self.texts.last().cloned()
    }

    pub fn stats(&self) -> (u32, u32) {
        (self.chunks_processed, self.chunks_failed)
    }
}

fn longest_common_word_overlap(prev: &str, curr: &str) -> Option<(usize, usize)> {
    let prev_lower = prev.to_lowercase();
    let curr_lower = curr.to_lowercase();

    let strip_punct = |s: &str| -> String {
        s.chars()
            .map(|c| if c.is_ascii_punctuation() { ' ' } else { c })
            .collect()
    };

    let prev_clean = strip_punct(&prev_lower);
    let curr_clean = strip_punct(&curr_lower);

    let prev_words: Vec<&str> = prev_clean.split_whitespace().collect();
    let curr_words: Vec<&str> = curr_clean.split_whitespace().collect();

    let plen = prev_words.len();
    let clen = curr_words.len();

    if plen < 2 || clen < 2 {
        return None;
    }

    // Find longest suffix of prev that matches a prefix of curr
    let max_overlap = plen.min(clen);
    for overlap_len in (2..=max_overlap).rev() {
        let prev_suffix = &prev_words[plen - overlap_len..];
        let curr_prefix = &curr_words[..overlap_len];
        if prev_suffix == curr_prefix {
            return Some((plen - overlap_len, overlap_len));
        }
    }

    None
}

pub struct RecordingHandle {
    pub is_recording: Arc<AtomicBool>,
    pub mic_peak: Arc<AtomicU32>,
    pub sys_peak: Arc<AtomicU32>,
    pub sys_active: bool,
    pub started_at: Instant,
    error_count: Arc<AtomicU32>,
    sample_rate: u32,
    sys_sample_rate: Option<u32>,
    mic_bluetooth: bool,
    language: String,
    denoise_enabled: bool,
    rx: mpsc::UnboundedReceiver<AudioChunk>,
    transcript: Arc<Mutex<ChunkedTranscript>>,
    all_samples_16k: Arc<Mutex<Vec<f32>>>,
    _audio_thread: std::thread::JoinHandle<()>,
    _chunker_thread: Option<std::thread::JoinHandle<()>>,
}

impl RecordingHandle {
    pub fn start(
        mic_device: Option<SelectedDevice>,
        sys_device: Option<SelectedDevice>,
        language: String,
    ) -> Result<Self> {
        let mic_bluetooth = mic_device
            .as_ref()
            .map(|d| is_likely_bluetooth(&d.name))
            .unwrap_or(false);
        if mic_bluetooth {
            eprintln!("[recording] Bluetooth mic detected, using larger buffers");
        }

        let is_recording = Arc::new(AtomicBool::new(true));
        let mic_peak = Arc::new(AtomicU32::new(0));
        let sys_peak = Arc::new(AtomicU32::new(0));
        let (tx, rx) = mpsc::unbounded_channel();

        let is_rec = is_recording.clone();
        let mic_pk = mic_peak.clone();
        let sys_pk = sys_peak.clone();
        let sys_tx = tx.clone();
        let (result_tx, result_rx) = std::sync::mpsc::channel();

        let audio_thread = std::thread::spawn(move || {
            match AudioCapture::start(tx, is_rec.clone(), mic_device, mic_pk, false) {
                Ok((_mic_stream, capture)) => {
                    let (_sys_stream, sys_started, sys_rate) = if let Some(sys_dev) = sys_device {
                        let sys_rec = is_rec.clone();
                        match AudioCapture::start(sys_tx, sys_rec, Some(sys_dev), sys_pk, true) {
                            Ok((stream, sys_capture)) => {
                                eprintln!("[recording] System audio stream started ({}Hz)", sys_capture.sample_rate);
                                (Some(stream), true, Some(sys_capture.sample_rate))
                            }
                            Err(e) => {
                                eprintln!("[recording] System audio failed (continuing with mic only): {e}");
                                (None, false, None)
                            }
                        }
                    } else {
                        (None, false, None)
                    };

                    let _ = result_tx.send(Ok((capture.sample_rate, sys_started, sys_rate)));
                    while is_rec.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
                Err(e) => {
                    let _ = result_tx.send(Err(e));
                }
            }
        });

        let (sample_rate, sys_active, sys_sample_rate) = result_rx
            .recv()
            .map_err(|_| anyhow::anyhow!("Audio thread died before reporting sample rate"))??;

        let transcript = Arc::new(Mutex::new(ChunkedTranscript::new()));
        let all_samples_16k = Arc::new(Mutex::new(Vec::<f32>::new()));

        Ok(RecordingHandle {
            is_recording,
            mic_peak,
            sys_peak,
            sys_active,
            started_at: Instant::now(),
            sample_rate,
            sys_sample_rate,
            mic_bluetooth,
            language,
            error_count: Arc::new(AtomicU32::new(0)),
            denoise_enabled: true,
            rx,
            transcript,
            all_samples_16k,
            _audio_thread: audio_thread,
            _chunker_thread: None,
        })
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    pub fn take_mic_peak(&self) -> f32 {
        f32::from_bits(self.mic_peak.swap(0, Ordering::Relaxed))
    }

    pub fn take_sys_peak(&self) -> f32 {
        f32::from_bits(self.sys_peak.swap(0, Ordering::Relaxed))
    }

    pub fn partial_transcript(&self) -> String {
        self.transcript.lock().expect("transcript mutex").merged()
    }

    pub fn chunk_stats(&self) -> (u32, u32) {
        self.transcript.lock().expect("transcript mutex").stats()
    }

    pub fn error_count(&self) -> u32 {
        self.error_count.load(Ordering::Relaxed)
    }

    pub fn stop_and_transcribe(
        mut self,
        whisper_ctx: &WhisperContext,
        audio_dir: &std::path::Path,
    ) -> Result<(String, PathBuf, f64)> {
        let duration_secs = self.elapsed_secs();

        // Grace period for very short recordings: if accumulated audio
        // is < 50ms at 16kHz (800 samples), wait up to 60ms for more
        // audio to arrive before stopping capture.
        const MIN_SAMPLES_16K: usize = 800;
        const GRACE_POLL_MS: u64 = 10;
        const GRACE_MAX_MS: u64 = 60;
        let sample_count = self.all_samples_16k.lock().expect("samples mutex").len();
        if sample_count < MIN_SAMPLES_16K {
            let deadline = Instant::now() + std::time::Duration::from_millis(GRACE_MAX_MS);
            while Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(GRACE_POLL_MS));
                let count = self.all_samples_16k.lock().expect("samples mutex").len();
                if count >= MIN_SAMPLES_16K {
                    break;
                }
            }
        }

        self.is_recording.store(false, Ordering::Relaxed);

        // Wait for the chunker thread to finish processing remaining
        // samples. Without this join, we race: the chunker polls every
        // 500ms and might not have written the final transcript yet.
        if let Some(thread) = self._chunker_thread.take() {
            let _ = thread.join();
        }

        // Drain remaining samples from channel (only has data when
        // chunker was never started)
        let mut remaining_raw: Vec<f32> = Vec::new();
        while let Ok(chunk) = self.rx.try_recv() {
            match chunk {
                AudioChunk::Mic(samples) | AudioChunk::System(samples) => {
                    remaining_raw.extend(samples);
                }
            }
        }

        // Collect all accumulated 16k samples
        let mut all_16k = {
            let guard = self.all_samples_16k.lock().expect("samples mutex");
            guard.clone()
        };

        if !remaining_raw.is_empty() {
            let mut hp = HighPassFilter::new(80.0, self.sample_rate);
            hp.process(&mut remaining_raw);
            let denoised = resample_to_48khz(&remaining_raw, self.sample_rate)
                .map(|mut s48| {
                    if self.denoise_enabled {
                        let mut dn = Denoiser::new();
                        dn.process(&mut s48);
                    }
                    let mut norm = LoudnessNormalizer::new(48000);
                    norm.process(&mut s48);
                    resample_to_16khz(&s48, 48000)
                });
            let resampled_result = match denoised {
                Ok(inner) => inner,
                Err(e) => Err(e),
            };
            if let Ok(resampled) = resampled_result {
                if !resampled.is_empty() {
                    // RMS silence check
                    let rms = (resampled.iter().map(|s| s * s).sum::<f32>()
                        / resampled.len() as f32)
                        .sqrt();
                    if rms >= 0.01 {
                        let prev = self.transcript.lock().expect("transcript mutex").last_chunk();
                        let vad_path = resolve_vad_path();
                        let text = transcription::transcribe_with_language(
                            whisper_ctx,
                            &resampled,
                            &self.language,
                            prev.as_deref(),
                            vad_path.as_deref(),
                        )
                        .unwrap_or_default();
                        self.transcript.lock().expect("transcript mutex").push(text);
                    }
                }
                all_16k.extend(resampled);
            }
        }

        if all_16k.is_empty() {
            return Err(anyhow::anyhow!("No audio captured"));
        }

        // Save full recording WAV
        std::fs::create_dir_all(audio_dir)?;
        let audio_path = audio_dir.join(format!("{}.wav", ulid::Ulid::new()));
        save_wav(&audio_path, &all_16k)?;

        let text = self.transcript.lock().expect("transcript mutex").merged();

        Ok((text, audio_path, duration_secs))
    }

    /// Start background chunker that drains audio samples every
    /// CHUNK_INTERVAL_SECS, resamples to 16kHz, and runs whisper
    /// inference on each chunk. Call after start() when whisper
    /// context is available.
    pub fn start_chunker(&mut self, whisper_ctx: Arc<WhisperContext>, denoise_enabled: bool) {
        self.denoise_enabled = denoise_enabled;
        let is_rec = self.is_recording.clone();
        let transcript = self.transcript.clone();
        let all_samples = self.all_samples_16k.clone();
        let error_count = self.error_count.clone();
        let sample_rate = self.sample_rate;
        let sys_sample_rate = self.sys_sample_rate;
        let mic_bluetooth = self.mic_bluetooth;
        let language = self.language.clone();
        let sys_active = self.sys_active;

        let rx = std::mem::replace(&mut self.rx, {
            let (_tx, rx) = mpsc::unbounded_channel();
            rx
        });

        let thread = std::thread::spawn(move || {
            chunker_loop(
                rx, is_rec, whisper_ctx, transcript, all_samples, error_count,
                sample_rate, sys_sample_rate, &language, sys_active, mic_bluetooth,
                denoise_enabled,
            );
        });

        self._chunker_thread = Some(thread);
    }
}

fn run_dsp(
    raw: &mut [f32],
    hp_filter: &mut HighPassFilter,
    denoiser: &mut Option<Denoiser>,
    normalizer: &mut LoudnessNormalizer,
    resampler_48k: &mut Option<PersistentResampler>,
    resampler_16k: &mut Option<PersistentResampler>,
    sample_rate: u32,
) -> Option<Vec<f32>> {
    hp_filter.process(raw);

    let mut s48 = if let Some(ref mut r) = resampler_48k {
        match r.process(raw) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[recording] resample to 48kHz failed: {e}");
                return None;
            }
        }
    } else {
        match resample_to_48khz(raw, sample_rate) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[recording] resample to 48kHz failed: {e}");
                return None;
            }
        }
    };

    if let Some(ref mut d) = denoiser {
        d.process(&mut s48);
    }
    normalizer.process(&mut s48);

    let s16 = if let Some(ref mut r) = resampler_16k {
        match r.process(&s48) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[recording] resample to 16kHz failed: {e}");
                return None;
            }
        }
    } else {
        match resample_to_16khz(&s48, 48000) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[recording] resample to 16kHz failed: {e}");
                return None;
            }
        }
    };

    Some(s16)
}

fn transcribe_segment(
    samples_16k: &[f32],
    whisper_ctx: &WhisperContext,
    transcript: &Mutex<ChunkedTranscript>,
    language: &str,
    use_whisper_vad: bool,
    stats: &mut ChunkerStats,
    error_count: &AtomicU32,
) {
    if samples_16k.is_empty() {
        return;
    }

    let rms = (samples_16k.iter().map(|s| s * s).sum::<f32>() / samples_16k.len() as f32).sqrt();
    if rms < 0.01 {
        stats.segments_skipped_silence += 1;
        return;
    }

    let prev = transcript.lock().expect("transcript mutex").last_chunk();
    let vad_path = if use_whisper_vad {
        resolve_vad_path()
    } else {
        None
    };
    match transcription::transcribe_with_language(
        whisper_ctx,
        samples_16k,
        language,
        prev.as_deref(),
        vad_path.as_deref(),
    ) {
        Ok(text) => {
            stats.segments_transcribed += 1;
            transcript.lock().expect("transcript mutex").push(text);
        }
        Err(e) => {
            eprintln!("[recording] segment transcription failed: {e}");
            error_count.fetch_add(1, Ordering::Relaxed);
            transcript
                .lock()
                .expect("transcript mutex")
                .record_failure();
        }
    }
}

fn process_and_route_chunks(
    rx: &mut mpsc::UnboundedReceiver<AudioChunk>,
    mixer: &mut AudioMixerRingBuffer,
    hp_filter: &mut HighPassFilter,
    denoiser: &mut Option<Denoiser>,
    normalizer: &mut LoudnessNormalizer,
    resampler_48k: &mut Option<PersistentResampler>,
    resampler_16k: &mut Option<PersistentResampler>,
    sample_rate: u32,
) {
    while let Ok(chunk) = rx.try_recv() {
        match chunk {
            AudioChunk::Mic(mut samples) => {
                if let Some(s16) = run_dsp(
                    &mut samples, hp_filter, denoiser, normalizer,
                    resampler_48k, resampler_16k, sample_rate,
                ) {
                    mixer.push_mic(&s16);
                }
            }
            AudioChunk::System(samples) => {
                mixer.push_system(&samples);
            }
        }
    }
}

fn process_vad_segments(
    segments: Vec<crate::audio::vad::SpeechSegment>,
    whisper_ctx: &WhisperContext,
    transcript: &Mutex<ChunkedTranscript>,
    language: &str,
    stats: &mut ChunkerStats,
    error_count: &AtomicU32,
) {
    for seg in segments {
        if seg.samples.len() >= 800 {
            transcribe_segment(&seg.samples, whisper_ctx, transcript, language, false, stats, error_count);
        } else {
            stats.segments_skipped_short += 1;
        }
    }
}

fn chunker_loop(
    mut rx: mpsc::UnboundedReceiver<AudioChunk>,
    is_recording: Arc<AtomicBool>,
    whisper_ctx: Arc<WhisperContext>,
    transcript: Arc<Mutex<ChunkedTranscript>>,
    all_samples_16k: Arc<Mutex<Vec<f32>>>,
    error_count: Arc<AtomicU32>,
    sample_rate: u32,
    sys_sample_rate: Option<u32>,
    language: &str,
    sys_active: bool,
    mic_bluetooth: bool,
    denoise_enabled: bool,
) {
    let mut hp_filter = HighPassFilter::new(80.0, sample_rate);
    let mut denoiser = if denoise_enabled { Some(Denoiser::new()) } else { None };
    let mut normalizer = LoudnessNormalizer::new(48000);
    let mut resampler_to_48k = match PersistentResampler::new(sample_rate, 48000) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("[recording] failed to create 48kHz resampler: {e}");
            None
        }
    };
    let mut resampler_to_16k = match PersistentResampler::new(48000, 16000) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("[recording] failed to create 16kHz resampler: {e}");
            None
        }
    };

    // Mixer at 16kHz: mic arrives pre-processed (DSP → 16k), system resampled internally
    let mut mixer = AudioMixerRingBuffer::with_bluetooth(16000, sys_sample_rate, sys_active, mic_bluetooth);

    let mut vad = match ContinuousVadProcessor::new(16000, VAD_REDEMPTION_TIME_MS) {
        Ok(v) => {
            eprintln!("[recording] VAD-driven chunking active ({}ms redemption)", VAD_REDEMPTION_TIME_MS);
            Some(v)
        }
        Err(e) => {
            eprintln!("[recording] VAD unavailable ({e}), using fixed-interval fallback");
            None
        }
    };

    let fallback_chunk_16k = (16000u64 * FALLBACK_CHUNK_SECS) as usize;
    let mut fallback_buffer: Vec<f32> = Vec::new();
    let mut stats = ChunkerStats::new();

    loop {
        process_and_route_chunks(
            &mut rx, &mut mixer,
            &mut hp_filter, &mut denoiser, &mut normalizer,
            &mut resampler_to_48k, &mut resampler_to_16k, sample_rate,
        );

        while let Some(mixed_16k) = mixer.extract_mixed() {
            all_samples_16k.lock().expect("samples mutex").extend(&mixed_16k);

            if let Some(ref mut vad_proc) = vad {
                match vad_proc.process_audio(&mixed_16k) {
                    Ok(segments) => {
                        process_vad_segments(segments, &whisper_ctx, &transcript, language, &mut stats, &error_count);
                    }
                    Err(e) => {
                        eprintln!("[recording] VAD error: {e}");
                        error_count.fetch_add(1, Ordering::Relaxed);
                    }
                }
            } else {
                fallback_buffer.extend(&mixed_16k);
                while fallback_buffer.len() >= fallback_chunk_16k {
                    let chunk_data: Vec<f32> =
                        fallback_buffer.drain(..fallback_chunk_16k).collect();
                    transcribe_segment(
                        &chunk_data, &whisper_ctx, &transcript, language, true, &mut stats, &error_count,
                    );
                }
            }
        }

        let errors = error_count.load(Ordering::Relaxed);
        if errors >= MAX_ERRORS {
            eprintln!("[recording] auto-stopping: {errors} errors exceeded threshold ({MAX_ERRORS})");
            is_recording.store(false, Ordering::Relaxed);
        }

        if !is_recording.load(Ordering::Relaxed) {
            // Final drain
            process_and_route_chunks(
                &mut rx, &mut mixer,
                &mut hp_filter, &mut denoiser, &mut normalizer,
                &mut resampler_to_48k, &mut resampler_to_16k, sample_rate,
            );
            let remaining_16k = mixer.drain_remaining();
            if !remaining_16k.is_empty() {
                all_samples_16k.lock().expect("samples mutex").extend(&remaining_16k);

                if let Some(ref mut vad_proc) = vad {
                    if let Ok(segments) = vad_proc.process_audio(&remaining_16k) {
                        process_vad_segments(segments, &whisper_ctx, &transcript, language, &mut stats, &error_count);
                    }
                } else {
                    fallback_buffer.extend(&remaining_16k);
                }
            }

            // Flush VAD or fallback remainder
            if let Some(ref mut vad_proc) = vad {
                match vad_proc.flush() {
                    Ok(segments) => {
                        process_vad_segments(segments, &whisper_ctx, &transcript, language, &mut stats, &error_count);
                    }
                    Err(e) => eprintln!("[recording] VAD flush error: {e}"),
                }
            } else if !fallback_buffer.is_empty() {
                let remainder = std::mem::take(&mut fallback_buffer);
                transcribe_segment(&remainder, &whisper_ctx, &transcript, language, true, &mut stats, &error_count);
            }

            break;
        }

        stats.maybe_report();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

pub fn save_transcription(
    store: &Store,
    text: String,
    audio_path: PathBuf,
    duration_secs: f64,
) -> Result<crate::store::Capture, String> {
    if text.trim().is_empty() {
        return Err("Transcription produced no text (silence or noise)".to_string());
    }
    store
        .save(CaptureInput::Transcription {
            text,
            audio_path,
            duration_secs,
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ChunkedTranscript ---

    #[test]
    fn transcript_new_is_empty() {
        let t = ChunkedTranscript::new();
        assert_eq!(t.merged(), "");
        assert_eq!(t.last_chunk(), None);
        assert_eq!(t.stats(), (0, 0));
    }

    #[test]
    fn transcript_push_non_empty_text() {
        let mut t = ChunkedTranscript::new();
        t.push("hello".into());
        t.push("world".into());
        assert_eq!(t.merged(), "hello world");
        assert_eq!(t.last_chunk(), Some("world".into()));
        assert_eq!(t.stats(), (2, 0));
    }

    #[test]
    fn transcript_push_empty_text_increments_count_but_skips_storage() {
        let mut t = ChunkedTranscript::new();
        t.push("hello".into());
        t.push("".into());
        t.push("world".into());
        assert_eq!(t.merged(), "hello world");
        assert_eq!(t.stats(), (3, 0));
    }

    #[test]
    fn transcript_record_failure_increments_failed_count() {
        let mut t = ChunkedTranscript::new();
        t.push("ok".into());
        t.record_failure();
        t.record_failure();
        assert_eq!(t.stats(), (1, 2));
    }

    #[test]
    fn transcript_merged_trims_whitespace() {
        let mut t = ChunkedTranscript::new();
        t.push("  hello  ".into());
        assert_eq!(t.merged(), "hello");
    }

    // --- validate_model_file ---

    #[test]
    fn validate_model_file_nonexistent_returns_false() {
        assert!(!validate_model_file(
            std::path::Path::new("/nonexistent/model.bin"),
            100
        ));
    }

    #[test]
    fn validate_model_file_too_small() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.bin");
        // GGML magic + 6 bytes = 10 bytes total
        let mut data = GGML_MAGIC.to_vec();
        data.extend_from_slice(&[0u8; 6]);
        std::fs::write(&path, &data).unwrap();
        assert!(!validate_model_file(&path, 1000));
    }

    #[test]
    fn validate_model_file_wrong_magic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad_magic.bin");
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x00, 0x00, 0x00];
        std::fs::write(&path, &data).unwrap();
        assert!(!validate_model_file(&path, 4));
    }

    #[test]
    fn validate_model_file_ggml_magic_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggml.bin");
        let mut data = GGML_MAGIC.to_vec();
        data.resize(100, 0);
        std::fs::write(&path, &data).unwrap();
        assert!(validate_model_file(&path, 50));
    }

    #[test]
    fn validate_model_file_gguf_magic_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gguf.bin");
        let mut data = GGUF_MAGIC.to_vec();
        data.resize(100, 0);
        std::fs::write(&path, &data).unwrap();
        assert!(validate_model_file(&path, 50));
    }

    // --- model paths ---

    #[test]
    fn model_path_ends_with_expected_filename() {
        let p = model_path();
        assert!(p.to_string_lossy().ends_with(MODEL_FILENAME));
    }

    #[test]
    fn vad_model_path_ends_with_expected_filename() {
        let p = vad_model_path();
        assert!(p.to_string_lossy().ends_with(VAD_MODEL_FILENAME));
    }

    #[test]
    fn audio_dir_is_under_data_dir() {
        let p = audio_dir();
        assert!(p.to_string_lossy().contains("com.koko.quick-capture"));
        assert!(p.to_string_lossy().ends_with("audio"));
    }

    #[test]
    fn overlap_dedup_no_overlap() {
        assert_eq!(longest_common_word_overlap("hello world", "foo bar"), None);
    }

    #[test]
    fn overlap_dedup_basic() {
        let result = longest_common_word_overlap("one two three four", "three four five six");
        assert_eq!(result, Some((2, 2)));
    }

    #[test]
    fn overlap_dedup_case_insensitive() {
        let result = longest_common_word_overlap("Hello World", "hello world again");
        assert_eq!(result, Some((0, 2)));
    }

    #[test]
    fn overlap_dedup_punctuation_insensitive() {
        let result = longest_common_word_overlap("end of sentence.", "sentence. Start of next");
        assert_eq!(result, None); // only 1 word overlap - below minimum
    }

    #[test]
    fn overlap_dedup_single_word_ignored() {
        assert_eq!(longest_common_word_overlap("foo bar", "bar baz"), None);
    }

    #[test]
    fn overlap_dedup_full_overlap() {
        let result = longest_common_word_overlap("a b", "a b");
        assert_eq!(result, Some((0, 2)));
    }

    #[test]
    fn chunked_transcript_dedup() {
        let mut t = ChunkedTranscript::new();
        t.push("one two three four".to_string());
        t.push("three four five six".to_string());
        assert_eq!(t.merged(), "one two three four five six");
    }

    #[test]
    fn chunked_transcript_no_overlap() {
        let mut t = ChunkedTranscript::new();
        t.push("hello world".to_string());
        t.push("foo bar".to_string());
        assert_eq!(t.merged(), "hello world foo bar");
    }
}
