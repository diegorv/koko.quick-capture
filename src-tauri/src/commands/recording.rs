use std::sync::Mutex;

use tauri::{AppHandle, Emitter, State};
use whisper_rs::WhisperContext;

use crate::audio::{DeviceType, SelectedDevice};
use crate::events::{CAPTURES_CHANGED as CAPTURES_CHANGED_EVENT, DOCK_PULSE as DOCK_PULSE_EVENT};
use crate::recording::{self, RecordingHandle};
use crate::store::{
    Store, SETTING_MIC_DEVICE, SETTING_SYS_AUDIO_DEVICE, SETTING_SYS_AUDIO_ENABLED,
    SETTING_TRANSCRIPTION_LANGUAGE,
};

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
    pub mic_peak: f32,
    pub sys_peak: f32,
    pub sys_active: bool,
    pub partial_transcript: String,
}

#[tauri::command]
pub fn list_audio_devices() -> Result<Vec<crate::audio::AudioDevice>, String> {
    crate::audio::list_input_devices().map_err(|e| e.to_string())
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
pub async fn start_recording(
    rec_state: State<'_, RecordingState>,
    whisper: State<'_, WhisperState>,
    store: State<'_, Store>,
) -> Result<(), String> {
    {
        let guard = rec_state.0.lock().expect("recording mutex poisoned");
        if guard.is_some() {
            return Err("Already recording".to_string());
        }
    }

    let whisper_clone = whisper.0.lock().expect("whisper mutex").clone();
    let ctx = if let Some(ctx) = whisper_clone {
        ctx
    } else {
        let path = recording::model_path();
        if !path.exists() {
            return Err("Model not downloaded. Call download_model first.".to_string());
        }
        let ctx = tauri::async_runtime::spawn_blocking(move || {
            crate::transcription::create_whisper_context(&path)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
        *whisper.0.lock().expect("whisper mutex") = Some(ctx.clone());
        ctx
    };

    let mic_device = store
        .settings_get(SETTING_MIC_DEVICE)
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
        .map(|name| SelectedDevice {
            name,
            device_type: DeviceType::Input,
        });

    let sys_enabled = store
        .settings_get(SETTING_SYS_AUDIO_ENABLED)
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);

    let sys_device = if sys_enabled {
        store
            .settings_get(SETTING_SYS_AUDIO_DEVICE)
            .ok()
            .flatten()
            .filter(|s| !s.is_empty())
            .map(|name| SelectedDevice {
                name,
                device_type: DeviceType::System,
            })
    } else {
        None
    };

    let language = store
        .settings_get(SETTING_TRANSCRIPTION_LANGUAGE)
        .ok()
        .flatten()
        .unwrap_or_else(|| crate::transcription::DEFAULT_LANGUAGE.to_string());

    let mut handle = RecordingHandle::start(mic_device, sys_device, language)
        .map_err(|e| e.to_string())?;
    handle.start_chunker(ctx);

    let mut guard = rec_state.0.lock().expect("recording mutex poisoned");
    *guard = Some(handle);
    Ok(())
}

#[tauri::command]
pub fn set_mic_device(
    name: Option<String>,
    store: State<'_, Store>,
) -> Result<(), String> {
    match name {
        Some(n) => store.settings_set(SETTING_MIC_DEVICE, &n),
        None => store.settings_set(SETTING_MIC_DEVICE, ""),
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_mic_device(store: State<'_, Store>) -> Result<Option<String>, String> {
    store
        .settings_get(SETTING_MIC_DEVICE)
        .map(|v| v.filter(|s| !s.is_empty()))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_transcription_language(
    language: String,
    store: State<'_, Store>,
) -> Result<(), String> {
    store
        .settings_set(SETTING_TRANSCRIPTION_LANGUAGE, &language)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_transcription_language(store: State<'_, Store>) -> Result<String, String> {
    store
        .settings_get(SETTING_TRANSCRIPTION_LANGUAGE)
        .map(|v| v.unwrap_or_else(|| crate::transcription::DEFAULT_LANGUAGE.to_string()))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_sys_audio_device(
    name: Option<String>,
    store: State<'_, Store>,
) -> Result<(), String> {
    match name {
        Some(n) => store.settings_set(SETTING_SYS_AUDIO_DEVICE, &n),
        None => store.settings_set(SETTING_SYS_AUDIO_DEVICE, ""),
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_sys_audio_device(store: State<'_, Store>) -> Result<Option<String>, String> {
    store
        .settings_get(SETTING_SYS_AUDIO_DEVICE)
        .map(|v| v.filter(|s| !s.is_empty()))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_sys_audio_enabled(
    enabled: bool,
    store: State<'_, Store>,
) -> Result<(), String> {
    store
        .settings_set(SETTING_SYS_AUDIO_ENABLED, if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_sys_audio_enabled(store: State<'_, Store>) -> Result<bool, String> {
    store
        .settings_get(SETTING_SYS_AUDIO_ENABLED)
        .map(|v| v.map(|s| s == "true").unwrap_or(false))
        .map_err(|e| e.to_string())
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
            mic_peak: handle.take_mic_peak(),
            sys_peak: handle.take_sys_peak(),
            sys_active: handle.sys_active,
            partial_transcript: handle.partial_transcript(),
        },
        None => RecordingStatus {
            active: false,
            elapsed_secs: 0.0,
            mic_peak: 0.0,
            sys_peak: 0.0,
            sys_active: false,
            partial_transcript: String::new(),
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

    let audio_dir = recording::audio_dir();

    let (text, audio_path, duration_secs) = tauri::async_runtime::spawn_blocking(move || {
        handle.stop_and_transcribe(&ctx, &audio_dir)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let transcript_text = text.clone();
    let capture = recording::save_transcription(&store, text, audio_path, duration_secs)?;
    let _ = app.emit(CAPTURES_CHANGED_EVENT, &capture);
    let _ = app.emit(DOCK_PULSE_EVENT, ());

    if let Ok(mut clip) = arboard::Clipboard::new() {
        let _ = clip.set_text(&transcript_text);
    }

    Ok(capture)
}
