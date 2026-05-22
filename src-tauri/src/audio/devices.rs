//! Audio device enumeration. Talks to cpal (CoreAudio for microphones,
//! ScreenCaptureKit for system audio on macOS 13+). All functions here
//! shell out to native APIs and cannot be unit-tested in isolation;
//! the type wrappers (`DeviceType`, `AudioDevice`, `SelectedDevice`)
//! and their serde shape are pure and visible to the rest of the
//! codebase as the public API for device selection persistence.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub enum DeviceType {
    Input,  // Microphone (CoreAudio)
    System, // System audio (ScreenCaptureKit)
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

/// List all microphone input devices (CoreAudio)
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

/// List system audio devices (ScreenCaptureKit) - macOS 13+
fn list_system_audio_devices() -> Result<Vec<AudioDevice>> {
    let mut devices = Vec::new();

    // Try to get ScreenCaptureKit host (may fail if not macOS 13+ or no permission)
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

/// List all available audio devices (microphones + system audio)
pub fn list_input_devices() -> Result<Vec<AudioDevice>> {
    let mut all_devices = list_microphone_devices()?;
    let system_devices = list_system_audio_devices().unwrap_or_default();
    all_devices.extend(system_devices);
    Ok(all_devices)
}

/// Resolve a named device through the appropriate cpal host. The
/// returned `bool` indicates whether the device was opened via the
/// ScreenCaptureKit host (i.e. is a system-audio source).
///
/// Visible to siblings inside `audio/` because the AudioCapture stream
/// builder needs it to attach the cpal stream callback in slice 5.
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
