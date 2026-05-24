use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use tokio::sync::mpsc;
use whisper_rs::WhisperContext;

use crate::audio::denoise::Denoiser;
use crate::audio::filter::HighPassFilter;
use crate::audio::normalize::LoudnessNormalizer;
use crate::audio::vad::ContinuousVadProcessor;
use crate::audio::{
    resample_to_16khz, resample_to_48khz, save_wav, AudioCapture, PersistentResampler,
    SelectedDevice,
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
        if !text.is_empty() {
            self.texts.push(text);
        }
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

pub struct RecordingHandle {
    pub is_recording: Arc<AtomicBool>,
    pub mic_peak: Arc<AtomicU32>,
    pub sys_peak: Arc<AtomicU32>,
    pub sys_active: bool,
    pub started_at: Instant,
    sample_rate: u32,
    language: String,
    rx: mpsc::UnboundedReceiver<Vec<f32>>,
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
            match AudioCapture::start(tx, is_rec.clone(), mic_device, mic_pk) {
                Ok((_mic_stream, capture)) => {
                    let (_sys_stream, sys_started) = if let Some(sys_dev) = sys_device {
                        let sys_rec = is_rec.clone();
                        match AudioCapture::start(sys_tx, sys_rec, Some(sys_dev), sys_pk) {
                            Ok((stream, _)) => {
                                eprintln!("[recording] System audio stream started");
                                (Some(stream), true)
                            }
                            Err(e) => {
                                eprintln!("[recording] System audio failed (continuing with mic only): {e}");
                                (None, false)
                            }
                        }
                    } else {
                        (None, false)
                    };

                    let _ = result_tx.send(Ok((capture.sample_rate, sys_started)));
                    while is_rec.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
                Err(e) => {
                    let _ = result_tx.send(Err(e));
                }
            }
        });

        let (sample_rate, sys_active) = result_rx
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
            language,
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

    pub fn stop_and_transcribe(
        mut self,
        whisper_ctx: &WhisperContext,
        audio_dir: &std::path::Path,
    ) -> Result<(String, PathBuf, f64)> {
        let duration_secs = self.elapsed_secs();

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
            remaining_raw.extend(chunk);
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
                    let mut dn = Denoiser::new();
                    dn.process(&mut s48);
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
    pub fn start_chunker(&mut self, whisper_ctx: Arc<WhisperContext>) {
        let is_rec = self.is_recording.clone();
        let transcript = self.transcript.clone();
        let all_samples = self.all_samples_16k.clone();
        let sample_rate = self.sample_rate;
        let language = self.language.clone();

        let rx = std::mem::replace(&mut self.rx, {
            let (_tx, rx) = mpsc::unbounded_channel();
            rx
        });

        let thread = std::thread::spawn(move || {
            chunker_loop(rx, is_rec, whisper_ctx, transcript, all_samples, sample_rate, &language);
        });

        self._chunker_thread = Some(thread);
    }
}

fn run_dsp(
    raw: &mut [f32],
    hp_filter: &mut HighPassFilter,
    denoiser: &mut Denoiser,
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

    denoiser.process(&mut s48);
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
) {
    if samples_16k.is_empty() {
        return;
    }

    let rms = (samples_16k.iter().map(|s| s * s).sum::<f32>() / samples_16k.len() as f32).sqrt();
    if rms < 0.01 {
        eprintln!("[recording] segment skipped (silence)");
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
            if !text.is_empty() {
                eprintln!(
                    "[recording] segment transcribed: {}...",
                    &text[..text.len().min(60)]
                );
            }
            transcript.lock().expect("transcript mutex").push(text);
        }
        Err(e) => {
            eprintln!("[recording] segment transcription failed: {e}");
            transcript
                .lock()
                .expect("transcript mutex")
                .record_failure();
        }
    }
}

fn chunker_loop(
    mut rx: mpsc::UnboundedReceiver<Vec<f32>>,
    is_recording: Arc<AtomicBool>,
    whisper_ctx: Arc<WhisperContext>,
    transcript: Arc<Mutex<ChunkedTranscript>>,
    all_samples_16k: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    language: &str,
) {
    let mut hp_filter = HighPassFilter::new(80.0, sample_rate);
    let mut denoiser = Denoiser::new();
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

    loop {
        let mut raw_batch: Vec<f32> = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            raw_batch.extend(chunk);
        }

        if !raw_batch.is_empty() {
            if let Some(samples_16k) = run_dsp(
                &mut raw_batch,
                &mut hp_filter,
                &mut denoiser,
                &mut normalizer,
                &mut resampler_to_48k,
                &mut resampler_to_16k,
                sample_rate,
            ) {
                {
                    let mut guard = all_samples_16k.lock().expect("samples mutex");
                    guard.extend(&samples_16k);
                }

                if let Some(ref mut vad_proc) = vad {
                    match vad_proc.process_audio(&samples_16k) {
                        Ok(segments) => {
                            for seg in segments {
                                if seg.samples.len() < 800 {
                                    continue;
                                }
                                transcribe_segment(
                                    &seg.samples,
                                    &whisper_ctx,
                                    &transcript,
                                    language,
                                    false,
                                );
                            }
                        }
                        Err(e) => eprintln!("[recording] VAD error: {e}"),
                    }
                } else {
                    fallback_buffer.extend(&samples_16k);
                    while fallback_buffer.len() >= fallback_chunk_16k {
                        let chunk_data: Vec<f32> =
                            fallback_buffer.drain(..fallback_chunk_16k).collect();
                        transcribe_segment(
                            &chunk_data,
                            &whisper_ctx,
                            &transcript,
                            language,
                            true,
                        );
                    }
                }
            }
        }

        if !is_recording.load(Ordering::Relaxed) {
            // Final drain
            let mut remaining: Vec<f32> = Vec::new();
            while let Ok(chunk) = rx.try_recv() {
                remaining.extend(chunk);
            }
            if !remaining.is_empty() {
                if let Some(samples_16k) = run_dsp(
                    &mut remaining,
                    &mut hp_filter,
                    &mut denoiser,
                    &mut normalizer,
                    &mut resampler_to_48k,
                    &mut resampler_to_16k,
                    sample_rate,
                ) {
                    all_samples_16k.lock().expect("samples mutex").extend(&samples_16k);

                    if let Some(ref mut vad_proc) = vad {
                        if let Ok(segments) = vad_proc.process_audio(&samples_16k) {
                            for seg in segments {
                                if seg.samples.len() >= 800 {
                                    transcribe_segment(
                                        &seg.samples,
                                        &whisper_ctx,
                                        &transcript,
                                        language,
                                        false,
                                    );
                                }
                            }
                        }
                    } else {
                        fallback_buffer.extend(&samples_16k);
                    }
                }
            }

            // Flush VAD or fallback remainder
            if let Some(ref mut vad_proc) = vad {
                match vad_proc.flush() {
                    Ok(segments) => {
                        for seg in segments {
                            if seg.samples.len() >= 800 {
                                transcribe_segment(
                                    &seg.samples,
                                    &whisper_ctx,
                                    &transcript,
                                    language,
                                    false,
                                );
                            }
                        }
                    }
                    Err(e) => eprintln!("[recording] VAD flush error: {e}"),
                }
            } else if !fallback_buffer.is_empty() {
                let remainder = std::mem::take(&mut fallback_buffer);
                transcribe_segment(&remainder, &whisper_ctx, &transcript, language, true);
            }

            break;
        }

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
