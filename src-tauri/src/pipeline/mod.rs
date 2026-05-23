//! Recording pipeline. The glue between `audio` (cpal capture +
//! resample + WAV I/O), `session` (manifest + chunk persistence), and
//! `transcription` (whisper-rs inference). Owns the two long-running
//! background tasks:
//!
//! - one capture thread + accumulation task per Track (started at app
//!   launch and kept alive for the process lifetime; restarts the
//!   cpal stream when the user switches devices)
//! - a single chunk-interval task that processes every Track's
//!   buffer every 5 minutes while recording is active.
//!
//! `process_track_chunk` is the per-chunk processor: drain buffer ->
//! resample -> save WAV -> update manifest -> silence-skip ->
//! transcribe -> append to active session's accumulated transcript
//! -> emit `chunk-transcribed` event. Reused by
//! `commands::recording::stop_recording_impl` to flush the final
//! chunk at stop time.
//!
//! Per ADR-0001 the orchestration here is smoke-only — the threads
//! talk to cpal and tokio runtimes and cannot be exercised in
//! isolation. Coverage is the same manual smoke flow as `audio/stream`.

use crate::audio;
use crate::session;
use crate::state::{AppState, TrackName};
use crate::transcription;
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager};
use tokio::sync::mpsc;

/// Process a single track's audio buffer: drain, resample, save WAV,
/// transcribe, emit event. Returns the transcript text on success.
pub async fn process_track_chunk(
    app: &tauri::AppHandle,
    track_name: TrackName,
) -> Result<String, String> {
    let state = app.state::<AppState>();

    let track = state
        .tracks
        .get(&track_name)
        .ok_or_else(|| format!("Unknown track: {}", track_name))?;

    // 1. Drain the buffer
    let audio_data = {
        let mut buffer = track.buffer.lock().await;
        std::mem::take(&mut *buffer)
    };

    if audio_data.is_empty() {
        return Err("Empty buffer".to_string());
    }

    let sample_rate = { *track.sample_rate.lock().await };

    // 2. Resample to 16kHz
    let resampled =
        audio::resample_to_16khz(&audio_data, sample_rate).map_err(|e| e.to_string())?;

    // 3. Get session info and determine chunk filename
    let (session_dir, chunk_filename) = {
        let mut session_guard = state.active_session.lock().await;
        let session = session_guard.as_mut().ok_or("No active session")?;
        let track_session = session
            .tracks
            .get_mut(&track_name)
            .ok_or_else(|| format!("Track {} not in session", track_name))?;
        let filename = format!("{}_{:03}.wav", track_name, track_session.chunk_index);
        track_session.chunk_index += 1;
        (session.session_dir.clone(), filename)
    };

    // 4. Save WAV to disk
    let wav_path = session_dir.join(&chunk_filename);
    audio::save_wav(&wav_path, &resampled).map_err(|e| e.to_string())?;

    // 5. Update session manifest with new chunk
    session::add_chunk_to_manifest(&session_dir, &chunk_filename, &track_name.to_string())
        .map_err(|e| e.to_string())?;

    let duration_secs = resampled.len() as f64 / 16000.0;
    eprintln!(
        "[chunk:{}] Saved {} ({:.1}s of audio)",
        track_name, chunk_filename, duration_secs
    );

    // 5b. Silence detection — skip transcription if audio is too quiet
    let rms = (resampled.iter().map(|s| s * s).sum::<f32>() / resampled.len() as f32).sqrt();
    if rms < 0.01 {
        eprintln!(
            "[chunk:{}] Silence detected (rms={:.6}), skipping transcription",
            track_name, rms
        );
        return Err("Silence".to_string());
    }

    // 6. Transcribe
    let _ = app.emit("transcription-status", format!("transcribing {}", track_name));

    let transcript = {
        let ctx_guard = state.whisper_context.lock().await;
        let ctx = ctx_guard.as_ref().ok_or("Whisper not initialized")?;
        transcription::transcribe(ctx, &resampled).map_err(|e| e.to_string())?
    };

    if transcript.is_empty() {
        eprintln!(
            "[chunk:{}] Empty/hallucinated transcript, skipping",
            track_name
        );
        return Err("No speech detected".to_string());
    }

    // 7. Update manifest with transcript
    session::update_chunk_transcript(&session_dir, &chunk_filename, &transcript)
        .map_err(|e| e.to_string())?;

    // 8. Accumulate transcript in active session
    {
        let mut session_guard = state.active_session.lock().await;
        if let Some(session) = session_guard.as_mut() {
            if let Some(track_session) = session.tracks.get_mut(&track_name) {
                if !track_session.accumulated_transcript.is_empty() {
                    track_session.accumulated_transcript.push(' ');
                }
                track_session.accumulated_transcript.push_str(&transcript);
            }
        }
    }

    // 9. Emit event with track info
    #[derive(serde::Serialize, Clone)]
    struct ChunkTranscribed {
        track: String,
        transcript: String,
    }
    let _ = app.emit(
        "chunk-transcribed",
        ChunkTranscribed {
            track: track_name.to_string(),
            transcript: transcript.clone(),
        },
    );

    eprintln!(
        "[chunk:{}] Transcribed: {}...",
        track_name,
        &transcript.chars().take(80).collect::<String>()
    );

    Ok(transcript)
}

/// Spawn one capture thread + accumulation task per Track, plus a
/// single chunk-interval task that processes every Track's buffer
/// every 5 minutes while recording is active. Call once from Tauri's
/// `setup` hook; the spawned tasks run for the process lifetime.
pub fn start(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let is_recording = state.is_recording.clone();

    // Spawn a capture thread + accumulation task for each track
    for (track_name, track) in &state.tracks {
        let buffer = track.buffer.clone();
        let sample_rate_mutex = track.sample_rate.clone();
        let device = track.device.clone();
        let mut change_rx = track.change_rx.clone();
        let is_rec = is_recording.clone();
        let use_default = track.use_default_when_none;
        let peak_level = track.peak_level.clone();
        let enabled = track.enabled.clone();
        let name = *track_name;

        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<f32>>();

        // Audio capture thread for this track
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(async move {
                let mut last_failure: Option<String> = None;
                loop {
                    let selected = { device.lock().await.clone() };
                    let is_enabled = enabled.load(Ordering::Relaxed);

                    // If disabled or no device and this track doesn't use default, wait for config
                    if !is_enabled || (selected.is_none() && !use_default) {
                        eprintln!(
                            "[audio:{}] {} waiting...",
                            name,
                            if !is_enabled {
                                "Disabled,"
                            } else {
                                "No device configured,"
                            }
                        );
                        let _ = change_rx.changed().await;
                        last_failure = None;
                        continue;
                    }

                    let sample_tx = tx.clone();
                    let is_rec_clone = is_rec.clone();

                    let capture_result = audio::AudioCapture::start(
                        sample_tx,
                        is_rec_clone,
                        selected.clone(),
                        peak_level.clone(),
                    );

                    match capture_result {
                        Ok((stream, capture)) => {
                            {
                                let mut rate = sample_rate_mutex.lock().await;
                                *rate = capture.sample_rate;
                            }
                            eprintln!(
                                "[audio:{}] Capture started: device={:?}, rate={}",
                                name, selected, capture.sample_rate
                            );
                            last_failure = None;

                            // Keep stream alive
                            let (stream_tx, stream_rx) = std::sync::mpsc::channel();
                            let _ = stream_tx.send(stream);

                            // Wait for device change
                            let _ = change_rx.changed().await;
                            eprintln!("[audio:{}] Device change, restarting...", name);

                            drop(stream_rx);
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            // Log only when the failure mode changes,
                            // so a disconnected configured device does
                            // not flood the log with the same line
                            // every 30s.
                            if last_failure.as_deref() != Some(&msg) {
                                eprintln!("[audio:{}] Failed to start: {}", name, msg);
                                last_failure = Some(msg);
                            }
                            // Long back-off + react instantly when the
                            // user changes the device in Settings:
                            // whichever fires first wins.
                            let timeout =
                                tokio::time::sleep(std::time::Duration::from_secs(30));
                            tokio::pin!(timeout);
                            tokio::select! {
                                _ = &mut timeout => {}
                                _ = change_rx.changed() => {}
                            }
                        }
                    }
                }
            });
        });

        // Accumulation task for this track
        let is_rec_acc = is_recording.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(samples) = rx.recv().await {
                if is_rec_acc.load(Ordering::Relaxed) {
                    let mut buf = buffer.lock().await;
                    buf.extend_from_slice(&samples);
                }
            }
        });
    }

    // Single periodic chunk processing task for all tracks
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let chunk_interval = std::time::Duration::from_secs(5 * 60);

        loop {
            // Wait until recording starts
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            let state = app_handle.state::<AppState>();
            if !state.is_recording.load(Ordering::Relaxed) {
                continue;
            }

            // Recording is active - wait for the chunk interval
            let start = std::time::Instant::now();
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if !state.is_recording.load(Ordering::Relaxed) {
                    break;
                }
                if start.elapsed() >= chunk_interval {
                    break;
                }
            }

            // Only process if still recording
            if state.is_recording.load(Ordering::Relaxed) {
                eprintln!("[chunk] 5-minute interval, processing all tracks...");
                let track_names: Vec<TrackName> = state.tracks.keys().copied().collect();
                for name in track_names {
                    let _ = process_track_chunk(&app_handle, name).await;
                }
            }
        }
    });
}
