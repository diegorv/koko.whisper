//! Audio domain entry point. Owns the per-submodule split:
//!
//! - `devices` — cpal host queries, device enumeration, type wrappers
//!   (`DeviceType`, `AudioDevice`, `SelectedDevice`).
//! - `resample` — pure DSP: mono mixdown and rubato resample to 16 kHz.
//! - `stream` — the cpal real-time capture callback (`AudioCapture`).
//!
//! WAV file I/O (`save_wav`, `load_wav`) lives here at the module root
//! because it does not split cleanly into devices/resample/stream and
//! is small enough to keep colocated.

use anyhow::Result;

mod devices;
mod resample;
mod stream;

pub use devices::{AudioDevice, DeviceType, SelectedDevice, list_input_devices};
pub use resample::resample_to_16khz;
pub use stream::AudioCapture;

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
