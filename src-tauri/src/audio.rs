use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

fn audio_to_mono(data: &[f32], channels: u16) -> Vec<f32> {
    if channels == 1 {
        return data.to_vec();
    }
    data.chunks(channels as usize)
        .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
        .collect()
}

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

fn find_device(name: &str, device_type: &DeviceType) -> Result<(cpal::Device, bool)> {
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

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SelectedDevice {
    pub name: String,
    pub device_type: DeviceType,
}

pub struct AudioCapture {
    pub sample_rate: u32,
}

impl AudioCapture {
    pub fn start(
        sample_sender: mpsc::UnboundedSender<Vec<f32>>,
        is_recording: Arc<AtomicBool>,
        selected: Option<SelectedDevice>,
        peak_level: Arc<AtomicU32>,
    ) -> Result<(cpal::Stream, Self)> {
        let device = if let Some(ref sel) = selected {
            let (dev, _is_sck) = find_device(&sel.name, &sel.device_type)?;
            dev
        } else {
            let host = cpal::default_host();
            host.default_input_device()
                .ok_or_else(|| anyhow!("No input device available"))?
        };

        eprintln!(
            "[audio] Using device: {}",
            device.name().unwrap_or_default()
        );

        let config = device.default_input_config()?;
        let channels = config.channels();
        let sample_rate = config.sample_rate().0;

        eprintln!(
            "[audio] Config: {} Hz, {} channels",
            sample_rate, channels
        );

        let stream = device.build_input_stream(
            &config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mono = audio_to_mono(data, channels);

                // Always compute peak for VU metering (even when not recording)
                let peak = mono.iter().fold(0.0f32, |max, &s| max.max(s.abs()));
                peak_level.fetch_max(peak.to_bits(), Ordering::Relaxed);

                if !is_recording.load(Ordering::Relaxed) {
                    return;
                }
                let _ = sample_sender.send(mono);
            },
            |err| {
                log::error!("Audio stream error: {}", err);
            },
            None,
        )?;

        stream.play()?;

        Ok((stream, AudioCapture { sample_rate }))
    }
}

/// Save 16kHz mono f32 audio to a WAV file
pub fn save_wav(path: &std::path::Path, samples: &[f32]) -> Result<()> {
    use std::io::BufWriter;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let file = std::fs::File::create(path)?;
    let buf_writer = BufWriter::new(file);
    let mut writer = hound::WavWriter::new(buf_writer, spec)?;
    for &sample in samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}

/// Load a 16kHz mono f32 WAV file back as samples
pub fn load_wav(path: &std::path::Path) -> Result<Vec<f32>> {
    let reader = hound::WavReader::open(path)?;
    let samples: Vec<f32> = reader
        .into_samples::<f32>()
        .collect::<std::result::Result<Vec<f32>, _>>()?;
    Ok(samples)
}

pub fn resample_to_16khz(samples: &[f32], from_rate: u32) -> Result<Vec<f32>> {
    if from_rate == 16000 {
        return Ok(samples.to_vec());
    }

    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType,
        WindowFunction,
    };

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        16000.0 / from_rate as f64,
        2.0,
        params,
        samples.len(),
        1,
    )?;

    let waves_in = vec![samples.to_vec()];
    let waves_out = resampler.process(&waves_in, None)?;
    Ok(waves_out.into_iter().next().unwrap())
}
