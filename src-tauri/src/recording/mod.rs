use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
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

/// Handle to a recording running on a dedicated thread.
/// `cpal::Stream` is `!Send`, so we keep the stream on the thread
/// that created it and communicate via atomics + channels.
pub struct RecordingHandle {
    pub is_recording: Arc<AtomicBool>,
    pub peak_level: Arc<AtomicU32>,
    pub started_at: Instant,
    sample_rate: u32,
    rx: mpsc::UnboundedReceiver<Vec<f32>>,
    _thread: std::thread::JoinHandle<()>,
}

// SAFETY: The `!Send` cpal::Stream lives on the spawned thread, not
// in this struct. Everything here is Send-safe.
unsafe impl Send for RecordingHandle {}

impl RecordingHandle {
    pub fn start() -> Result<Self> {
        let is_recording = Arc::new(AtomicBool::new(true));
        let peak_level = Arc::new(AtomicU32::new(0));
        let (tx, rx) = mpsc::unbounded_channel();

        let is_rec = is_recording.clone();
        let peak = peak_level.clone();

        let (result_tx, result_rx) = std::sync::mpsc::channel();

        let thread = std::thread::spawn(move || {
            match AudioCapture::start(tx, is_rec.clone(), None, peak) {
                Ok((_stream, capture)) => {
                    let _ = result_tx.send(Ok(capture.sample_rate));
                    // Keep thread alive while recording — stream is dropped
                    // when this thread exits.
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

        Ok(RecordingHandle {
            is_recording,
            peak_level,
            started_at: Instant::now(),
            sample_rate,
            rx,
            _thread: thread,
        })
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    pub fn take_peak(&self) -> f32 {
        let bits = self.peak_level.swap(0, Ordering::Relaxed);
        f32::from_bits(bits)
    }

    pub fn stop_and_transcribe(
        mut self,
        whisper_ctx: &WhisperContext,
        audio_dir: &std::path::Path,
    ) -> Result<(String, PathBuf, f64)> {
        let duration_secs = self.elapsed_secs();

        self.is_recording.store(false, Ordering::Relaxed);

        // Give the audio thread a moment to flush remaining samples
        std::thread::sleep(std::time::Duration::from_millis(200));

        let mut all_samples: Vec<f32> = Vec::new();
        while let Ok(chunk) = self.rx.try_recv() {
            all_samples.extend(chunk);
        }

        if all_samples.is_empty() {
            return Err(anyhow::anyhow!("No audio captured"));
        }

        let resampled = resample_to_16khz(&all_samples, self.sample_rate)?;

        std::fs::create_dir_all(audio_dir)?;
        let audio_path = audio_dir.join(format!(
            "{}.wav",
            ulid::Ulid::new()
        ));
        save_wav(&audio_path, &resampled)?;

        let text = transcription::transcribe(whisper_ctx, &resampled)?;

        Ok((text, audio_path, duration_secs))
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
