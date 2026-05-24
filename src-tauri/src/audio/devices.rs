use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub enum DeviceType {
    Input,
    System,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct AudioDevice {
    pub name: String,
    pub device_type: DeviceType,
    pub is_default: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SelectedDevice {
    pub name: String,
    pub device_type: DeviceType,
}

fn list_microphone_devices() -> Result<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let mut devices = Vec::new();
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            devices.push(AudioDevice {
                is_default: name == default_name,
                name,
                device_type: DeviceType::Input,
            });
        }
    }
    Ok(devices)
}

fn list_system_audio_devices() -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();
    match cpal::host_from_id(cpal::HostId::ScreenCaptureKit) {
        Ok(sck_host) => {
            if let Ok(input_devices) = sck_host.input_devices() {
                for device in input_devices {
                    if let Ok(name) = device.name() {
                        devices.push(AudioDevice {
                            is_default: false,
                            name,
                            device_type: DeviceType::System,
                        });
                    }
                }
            }
        }
        Err(e) => {
            eprintln!(
                "[audio] ScreenCaptureKit not available: {}. Need macOS 13+ and Screen Recording permission.",
                e
            );
        }
    }
    Ok(devices)
}

pub fn list_input_devices() -> Result<Vec<AudioDevice>> {
    let mut all_devices = list_microphone_devices()?;
    let system_devices = list_system_audio_devices().unwrap_or_default();
    all_devices.extend(system_devices);
    Ok(all_devices)
}

pub(super) fn find_device(name: &str, device_type: &DeviceType) -> Result<(cpal::Device, bool)> {
    match device_type {
        DeviceType::Input => {
            let host = cpal::default_host();
            for device in host.input_devices()? {
                if let Ok(n) = device.name() {
                    if n == name {
                        return Ok((device, false));
                    }
                }
            }
            Err(anyhow!("Input device '{}' not found", name))
        }
        DeviceType::System => {
            let host = cpal::host_from_id(cpal::HostId::ScreenCaptureKit)
                .map_err(|e| anyhow!("ScreenCaptureKit not available: {}", e))?;
            for device in host.input_devices()? {
                if let Ok(n) = device.name() {
                    if n == name {
                        return Ok((device, true));
                    }
                }
            }
            Err(anyhow!("System audio device '{}' not found", name))
        }
    }
}
