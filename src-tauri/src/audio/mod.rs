//! Audio domain entry point. Owns the per-submodule split:
//!
//! - `devices` — cpal host queries, device enumeration, type wrappers
//!   (`DeviceType`, `AudioDevice`, `SelectedDevice`).
//! - `resample` — pure DSP: mono mixdown and rubato resample to 16 kHz.
//! - (slice 5) `stream` — the cpal real-time capture callback.
//!
//! WAV file I/O (`save_wav`, `load_wav`) lives here for now; it does
//! not split cleanly into devices/resample/stream and is small enough
//! to keep at the module root.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

mod devices;
mod resample;

pub use devices::{AudioDevice, DeviceType, SelectedDevice, list_input_devices};
pub use resample::resample_to_16khz;

use devices::find_device;
use resample::audio_to_mono;

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
