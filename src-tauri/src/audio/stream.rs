//! Real-time cpal capture stream. Spawns a native audio callback that
//! runs on cpal's stream thread (not the tokio runtime), mixes
//! interleaved frames to mono, updates a VU peak meter, and forwards
//! the mono buffer to the recording pipeline via an mpsc channel
//! while `is_recording` is set.
//!
//! Per ADR-0001 this module is the canonical example of a slice with
//! no pure surface to unit-test. The cpal callback closes over native
//! state (the device, the stream, the kernel ring buffer), runs on a
//! real-time thread, and cannot be exercised without an actual audio
//! device. Coverage is the manual smoke test: start a recording from
//! `/`, watch the VU meter respond, verify a session directory and
//! WAVs land on disk, verify the transcript appears.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::devices::{find_device, SelectedDevice};
use super::resample::audio_to_mono;

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
            None,
        )?;

        stream.play()?;

        Ok((stream, AudioCapture { sample_rate }))
    }
}
