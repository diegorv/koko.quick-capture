use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use tokio::sync::mpsc;
use whisper_rs::WhisperContext;

use crate::audio::{resample_to_16khz, save_wav, AudioCapture};
use crate::store::{CaptureInput, Store};
use crate::transcription;

const MODEL_FILENAME: &str = "ggml-large-v3-turbo-q5_0.bin";
const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin";

const CHUNK_INTERVAL_SECS: u64 = 30;

pub fn models_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("com.koko.quick-capture").join("models")
}

pub fn model_path() -> PathBuf {
    models_dir().join(MODEL_FILENAME)
}

pub fn is_model_downloaded() -> bool {
    model_path().exists()
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

    if path.exists() {
        return Ok(path);
    }

    let tmp_path = dir.join(format!("{MODEL_FILENAME}.tmp"));

    let resp = reqwest::get(MODEL_URL).await?;
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
    Ok(path)
}

/// Accumulated transcript chunks from the background chunker thread.
pub struct ChunkedTranscript {
    texts: Vec<String>,
}

impl ChunkedTranscript {
    fn new() -> Self {
        Self { texts: Vec::new() }
    }

    fn push(&mut self, text: String) {
        if !text.is_empty() {
            self.texts.push(text);
        }
    }

    fn merged(&self) -> String {
        self.texts.join(" ").trim().to_string()
    }
}

pub struct RecordingHandle {
    pub is_recording: Arc<AtomicBool>,
    pub peak_level: Arc<AtomicU32>,
    pub started_at: Instant,
    sample_rate: u32,
    rx: mpsc::UnboundedReceiver<Vec<f32>>,
    transcript: Arc<Mutex<ChunkedTranscript>>,
    all_samples_16k: Arc<Mutex<Vec<f32>>>,
    _audio_thread: std::thread::JoinHandle<()>,
    _chunker_thread: Option<std::thread::JoinHandle<()>>,
}

unsafe impl Send for RecordingHandle {}

impl RecordingHandle {
    pub fn start(_whisper_ctx: Option<Arc<WhisperContext>>) -> Result<Self> {
        let is_recording = Arc::new(AtomicBool::new(true));
        let peak_level = Arc::new(AtomicU32::new(0));
        let (tx, rx) = mpsc::unbounded_channel();

        let is_rec = is_recording.clone();
        let peak = peak_level.clone();

        let (result_tx, result_rx) = std::sync::mpsc::channel();

        let audio_thread = std::thread::spawn(move || {
            match AudioCapture::start(tx, is_rec.clone(), None, peak) {
                Ok((_stream, capture)) => {
                    let _ = result_tx.send(Ok(capture.sample_rate));
                    while is_rec.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
                Err(e) => {
                    let _ = result_tx.send(Err(e));
                }
            }
        });

        let sample_rate = result_rx
            .recv()
            .map_err(|_| anyhow::anyhow!("Audio thread died before reporting sample rate"))??;

        let transcript = Arc::new(Mutex::new(ChunkedTranscript::new()));
        let all_samples_16k = Arc::new(Mutex::new(Vec::<f32>::new()));

        Ok(RecordingHandle {
            is_recording,
            peak_level,
            started_at: Instant::now(),
            sample_rate,
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

    pub fn take_peak(&self) -> f32 {
        let bits = self.peak_level.swap(0, Ordering::Relaxed);
        f32::from_bits(bits)
    }

    pub fn partial_transcript(&self) -> String {
        self.transcript.lock().expect("transcript mutex").merged()
    }

    pub fn stop_and_transcribe(
        mut self,
        whisper_ctx: &WhisperContext,
        audio_dir: &std::path::Path,
    ) -> Result<(String, PathBuf, f64)> {
        let duration_secs = self.elapsed_secs();

        self.is_recording.store(false, Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Drain remaining samples from channel
        let mut remaining_raw: Vec<f32> = Vec::new();
        while let Ok(chunk) = self.rx.try_recv() {
            remaining_raw.extend(chunk);
        }

        // Collect all accumulated 16k samples
        let mut all_16k = {
            let guard = self.all_samples_16k.lock().expect("samples mutex");
            guard.clone()
        };

        // Resample and transcribe any remaining raw samples
        if !remaining_raw.is_empty() {
            if let Ok(resampled) = resample_to_16khz(&remaining_raw, self.sample_rate) {
                // RMS silence check
                let rms = (resampled.iter().map(|s| s * s).sum::<f32>()
                    / resampled.len() as f32)
                    .sqrt();
                if rms >= 0.01 {
                    let text = transcription::transcribe(whisper_ctx, &resampled)
                        .unwrap_or_default();
                    self.transcript.lock().expect("transcript mutex").push(text);
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

        // Move the receiver to the chunker thread so it owns the
        // drain loop.
        let rx = std::mem::replace(&mut self.rx, {
            let (_tx, rx) = mpsc::unbounded_channel();
            rx
        });

        let thread = std::thread::spawn(move || {
            chunker_loop(rx, is_rec, whisper_ctx, transcript, all_samples, sample_rate);
        });

        self._chunker_thread = Some(thread);
    }
}

fn chunker_loop(
    mut rx: mpsc::UnboundedReceiver<Vec<f32>>,
    is_recording: Arc<AtomicBool>,
    whisper_ctx: Arc<WhisperContext>,
    transcript: Arc<Mutex<ChunkedTranscript>>,
    all_samples_16k: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
) {
    let mut buffer: Vec<f32> = Vec::new();
    let chunk_samples = (sample_rate as u64 * CHUNK_INTERVAL_SECS) as usize;

    loop {
        // Drain available samples (non-blocking)
        while let Ok(chunk) = rx.try_recv() {
            buffer.extend(chunk);
        }

        // Process chunk if we have enough samples
        if buffer.len() >= chunk_samples {
            let chunk_data: Vec<f32> = buffer.drain(..chunk_samples).collect();
            process_chunk(
                &chunk_data,
                sample_rate,
                &whisper_ctx,
                &transcript,
                &all_samples_16k,
            );
        }

        if !is_recording.load(Ordering::Relaxed) {
            // Process remaining buffer
            if !buffer.is_empty() {
                process_chunk(
                    &buffer,
                    sample_rate,
                    &whisper_ctx,
                    &transcript,
                    &all_samples_16k,
                );
            }
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

fn process_chunk(
    raw_samples: &[f32],
    sample_rate: u32,
    whisper_ctx: &WhisperContext,
    transcript: &Mutex<ChunkedTranscript>,
    all_samples_16k: &Mutex<Vec<f32>>,
) {
    let resampled = match resample_to_16khz(raw_samples, sample_rate) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[recording] resample failed: {e}");
            return;
        }
    };

    {
        let mut guard = all_samples_16k.lock().expect("samples mutex");
        guard.extend(&resampled);
    }

    // RMS silence check
    let rms = (resampled.iter().map(|s| s * s).sum::<f32>() / resampled.len() as f32).sqrt();
    if rms < 0.01 {
        eprintln!("[recording] chunk skipped (silence)");
        return;
    }

    match transcription::transcribe(whisper_ctx, &resampled) {
        Ok(text) => {
            if !text.is_empty() {
                eprintln!("[recording] chunk transcribed: {}...", &text[..text.len().min(60)]);
                transcript.lock().expect("transcript mutex").push(text);
            }
        }
        Err(e) => {
            eprintln!("[recording] chunk transcription failed: {e}");
        }
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
