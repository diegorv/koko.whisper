//! Pure DSP. Mono mixdown and sample-rate conversion to the 16 kHz
//! whisper-rs expects. No I/O, no global state — safe to call from
//! the cpal real-time callback (mono mixdown) or from the post-capture
//! async pipeline (resample).

use anyhow::Result;

/// Mix interleaved multi-channel audio down to mono. Mono input is
/// returned unchanged (allocation included — caller owns the buffer).
///
/// `channels = 0` is invalid by cpal contract and would divide by
/// zero; the function does not guard it because cpal never reports it.
pub(super) fn audio_to_mono(data: &[f32], channels: u16) -> Vec<f32> {
    if channels == 1 {
        return data.to_vec();
    }
    data.chunks(channels as usize)
        .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Resample `samples` from `from_rate` to 16 kHz. Source rates equal
/// to 16 kHz short-circuit and return a clone; everything else runs
/// through rubato's async sinc resampler.
pub fn resample_to_16khz(samples: &[f32], from_rate: u32) -> Result<Vec<f32>> {
    if from_rate == 16000 {
        return Ok(samples.to_vec());
    }

    use rubato::{
        Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
        WindowFunction,
    };

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = Async::<f32>::new_sinc(
        16000.0 / from_rate as f64,
        2.0,
        &params,
        samples.len(),
        1,
        FixedAsync::Input,
    )?;

    use audioadapter_buffers::direct::SequentialSliceOfVecs;

    let waves_in_data = vec![samples.to_vec()];
    let waves_in = SequentialSliceOfVecs::new(&waves_in_data[..], 1, samples.len())
        .map_err(|e| anyhow::anyhow!("Failed to create audio buffer: {}", e))?;
    let waves_out = resampler.process(&waves_in, 0, None)?;
    Ok(waves_out.take_data())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_to_mono_mono_input_is_returned_as_clone() {
        let input = vec![0.1, 0.2, 0.3];
        let out = audio_to_mono(&input, 1);
        assert_eq!(out, input);
    }

    #[test]
    fn audio_to_mono_stereo_averages_each_pair() {
        // Interleaved L/R: (1, -1), (2, 0), (4, 2) -> averages 0, 1, 3
        let input = vec![1.0, -1.0, 2.0, 0.0, 4.0, 2.0];
        let out = audio_to_mono(&input, 2);
        assert_eq!(out, vec![0.0, 1.0, 3.0]);
    }

    #[test]
    fn audio_to_mono_four_channels_averages_every_quad() {
        // Two frames of 4 channels each.
        let input = vec![
            1.0, 2.0, 3.0, 4.0, // frame 0 -> (1+2+3+4)/4 = 2.5
            0.0, 0.0, 0.0, 0.0, // frame 1 -> 0
        ];
        let out = audio_to_mono(&input, 4);
        assert_eq!(out, vec![2.5, 0.0]);
    }

    #[test]
    fn audio_to_mono_empty_input_returns_empty() {
        let input: Vec<f32> = Vec::new();
        assert!(audio_to_mono(&input, 1).is_empty());
        assert!(audio_to_mono(&input, 2).is_empty());
    }

    #[test]
    fn resample_to_16khz_is_a_clone_when_already_16khz() {
        let input = vec![0.1f32, 0.2, 0.3, 0.4];
        let out = resample_to_16khz(&input, 16000).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn resample_to_16khz_48k_to_16k_produces_roughly_one_third_samples() {
        // Synthesize a moderately long buffer so rubato's sinc kernel
        // has room to work (the default sinc_len = 256 needs at least
        // a few hundred input samples before it produces output).
        let input: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.01).sin()).collect();
        let out = resample_to_16khz(&input, 48000).unwrap();

        // 48k -> 16k is a 3:1 downsample. Allow a generous ±5%
        // tolerance because rubato's async sinc trims a few samples
        // off the head/tail.
        let expected = input.len() / 3;
        let low = (expected as f32 * 0.95) as usize;
        let high = (expected as f32 * 1.05) as usize;
        assert!(
            out.len() >= low && out.len() <= high,
            "expected output length in [{}, {}], got {}",
            low,
            high,
            out.len()
        );
    }
}
