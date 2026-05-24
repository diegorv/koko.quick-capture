use std::sync::Mutex;

use tauri::{AppHandle, Emitter, State};
use whisper_rs::WhisperContext;

use crate::events::{CAPTURES_CHANGED as CAPTURES_CHANGED_EVENT, DOCK_PULSE as DOCK_PULSE_EVENT};
use crate::recording::{self, RecordingHandle};
use crate::store::Store;

pub struct RecordingState(pub Mutex<Option<RecordingHandle>>);

pub struct WhisperState(pub Mutex<Option<std::sync::Arc<WhisperContext>>>);

#[derive(serde::Serialize, Clone)]
pub struct ModelStatus {
    pub downloaded: bool,
    pub path: String,
}

#[derive(serde::Serialize, Clone)]
pub struct RecordingStatus {
    pub active: bool,
    pub elapsed_secs: f64,
    pub peak_level: f32,
}

#[tauri::command]
pub fn get_model_status() -> ModelStatus {
    ModelStatus {
        downloaded: recording::is_model_downloaded(),
        path: recording::model_path().to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub async fn download_model(app: AppHandle) -> Result<ModelStatus, String> {
    recording::download_model(|downloaded, total| {
        let _ = app.emit("model:download-progress", (downloaded, total));
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(ModelStatus {
        downloaded: true,
        path: recording::model_path().to_string_lossy().to_string(),
    })
}

fn ensure_whisper_loaded(whisper: &WhisperState) -> Result<std::sync::Arc<WhisperContext>, String> {
    let mut guard = whisper.0.lock().expect("whisper mutex poisoned");
    if let Some(ctx) = guard.as_ref() {
        return Ok(ctx.clone());
    }
    let path = recording::model_path();
    if !path.exists() {
        return Err("Model not downloaded. Call download_model first.".to_string());
    }
    let ctx = crate::transcription::create_whisper_context(&path)
        .map_err(|e| e.to_string())?;
    *guard = Some(ctx.clone());
    Ok(ctx)
}

#[tauri::command]
pub fn start_recording(
    rec_state: State<'_, RecordingState>,
) -> Result<(), String> {
    let mut guard = rec_state.0.lock().expect("recording mutex poisoned");
    if guard.is_some() {
        return Err("Already recording".to_string());
    }
    let handle = RecordingHandle::start().map_err(|e| e.to_string())?;
    *guard = Some(handle);
    Ok(())
}

#[tauri::command]
pub fn get_recording_status(
    rec_state: State<'_, RecordingState>,
) -> RecordingStatus {
    let guard = rec_state.0.lock().expect("recording mutex poisoned");
    match guard.as_ref() {
        Some(handle) => RecordingStatus {
            active: true,
            elapsed_secs: handle.elapsed_secs(),
            peak_level: handle.take_peak(),
        },
        None => RecordingStatus {
            active: false,
            elapsed_secs: 0.0,
            peak_level: 0.0,
        },
    }
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    rec_state: State<'_, RecordingState>,
    whisper: State<'_, WhisperState>,
    store: State<'_, Store>,
) -> Result<crate::store::Capture, String> {
    let ctx = ensure_whisper_loaded(&whisper)?;

    let handle = {
        let mut guard = rec_state.0.lock().expect("recording mutex poisoned");
        guard.take().ok_or("Not recording")?
    };

    let audio_dir = recording::model_path()
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("audio"))
        .unwrap_or_else(|| std::path::PathBuf::from("audio"));

    let (text, audio_path, duration_secs) = tauri::async_runtime::spawn_blocking(move || {
        handle.stop_and_transcribe(&ctx, &audio_dir)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let capture = recording::save_transcription(&store, text, audio_path, duration_secs)?;
    let _ = app.emit(CAPTURES_CHANGED_EVENT, &capture);
    let _ = app.emit(DOCK_PULSE_EVENT, ());
    Ok(capture)
}
