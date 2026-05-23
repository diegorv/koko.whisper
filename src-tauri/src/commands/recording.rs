//! Recording lifecycle Tauri commands. start / stop / toggle, plus
//! the audio-level meter polled by the frontend and the
//! `build_transcript` aggregator that produces the final markdown
//! body once a session ends.
//!
//! `*_impl` variants of start/stop are the actual logic; the
//! `#[tauri::command]` wrappers above them exist only to satisfy
//! Tauri's IPC signature requirements (owned `AppHandle` instead of
//! `&AppHandle`). They are called by the same name from
//! `tray::handle_menu_event` and `shortcuts::register` through the
//! re-exported path `crate::commands::toggle_recording_impl`.

use crate::state::{
    ActiveSession, ActiveTrackSession, AppState, TrackName, STATUS_IDLE, STATUS_RECORDING,
    STATUS_TRANSCRIBING,
};
use chrono::Local;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

/// Build final transcript from per-track transcripts.
/// Single track: plain text. Multiple tracks: markdown headers.
pub fn build_transcript(track_transcripts: &HashMap<TrackName, String>) -> String {
    let non_empty: Vec<_> = track_transcripts
        .iter()
        .filter(|(_, t)| !t.is_empty())
        .collect();

    if non_empty.len() <= 1 {
        return non_empty
            .into_iter()
            .map(|(_, t)| t.clone())
            .next()
            .unwrap_or_default();
    }

    // Multiple tracks: use headers in defined order
    let ordered = [TrackName::Microphone, TrackName::System];
    let mut parts = Vec::new();
    for track in &ordered {
        if let Some(text) = track_transcripts.get(track) {
            if !text.is_empty() {
                parts.push(format!("## {}\n\n{}", track.display_label(), text));
            }
        }
    }
    parts.join("\n\n")
}

pub(super) fn copy_to_clipboard(text: &str) {
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => {
            if let Err(e) = clipboard.set_text(text) {
                eprintln!("[clipboard] Failed to copy: {}", e);
            }
        }
        Err(e) => {
            eprintln!("[clipboard] Failed to access clipboard: {}", e);
        }
    }
}

/// Extra metadata woven into the saved markdown frontmatter. All
/// fields are optional: any `None` row is omitted, so legacy code
/// paths (or the recovery flow that has no clean duration) write
/// only the fields they know about.
#[derive(Default, Clone)]
pub(super) struct TranscriptMeta {
    pub duration_seconds: Option<u64>,
    pub mic_device: Option<String>,
    pub sys_device: Option<String>,
    pub chunks: Option<u32>,
}

fn format_duration(seconds: u64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

/// Pure. Build a `TranscriptMeta` from a (just-completed or just-
/// recovered) session manifest. Devices and chunk count come from
/// the manifest; the caller supplies `duration_seconds` because the
/// manifest does not record session length on disk (it would force
/// rewriting the manifest after stop, and recovery has no clean
/// duration anyway).
pub(super) fn build_meta_from_manifest(
    manifest: &crate::session::SessionManifest,
    duration_seconds: Option<u64>,
) -> TranscriptMeta {
    let mic_device = manifest
        .tracks
        .get(&TrackName::Microphone.to_string())
        .map(|info| info.device_name.clone());
    let sys_device = manifest
        .tracks
        .get(&TrackName::System.to_string())
        .map(|info| info.device_name.clone());
    let chunks = if manifest.chunks.is_empty() {
        None
    } else {
        Some(manifest.chunks.len() as u32)
    };
    TranscriptMeta {
        duration_seconds,
        mic_device,
        sys_device,
        chunks,
    }
}

/// Pure. Build the markdown frontmatter block from the wall-clock
/// timestamp and the optional meta rows. The body separator (`---`)
/// and the transcript itself are appended by the caller.
pub(super) fn build_frontmatter(now_str: &str, meta: &TranscriptMeta) -> String {
    let mut lines: Vec<String> = vec![
        "# Voice transcription".to_string(),
        String::new(),
        format!("**Date:** {}", now_str),
    ];
    if let Some(dur) = meta.duration_seconds {
        lines.push(format!("**Duration:** {}", format_duration(dur)));
    }
    lines.push("**Language:** Portuguese (BR)".to_string());
    if let Some(ref mic) = meta.mic_device {
        lines.push(format!("**Microphone:** {}", mic));
    }
    if let Some(ref sys) = meta.sys_device {
        lines.push(format!("**System audio:** {}", sys));
    }
    if let Some(chunks) = meta.chunks {
        lines.push(format!("**Chunks:** {}", chunks));
    }
    lines.join("\n")
}

pub(super) fn save_markdown(
    output_folder: &PathBuf,
    transcript: &str,
    meta: &TranscriptMeta,
) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(output_folder)?;
    let now = Local::now();
    let filename = format!("{}.md", now.format("%Y-%m-%d_%H-%M-%S"));
    let file_path = output_folder.join(&filename);

    let frontmatter = build_frontmatter(&now.format("%Y-%m-%d %H:%M:%S").to_string(), meta);
    let content = format!("{}\n\n---\n\n{}\n", frontmatter, transcript);

    std::fs::write(&file_path, content)?;
    log::info!("Saved transcription to {:?}", file_path);
    Ok(file_path)
}

#[tauri::command]
pub async fn get_audio_levels(
    state: State<'_, AppState>,
) -> Result<HashMap<String, f32>, String> {
    let mut levels = HashMap::new();
    for (name, track) in &state.tracks {
        let bits = track.peak_level.swap(0, Ordering::Relaxed);
        let level = f32::from_bits(bits).clamp(0.0, 1.0);
        levels.insert(name.to_string(), level);
    }
    Ok(levels)
}

/// Core recording start logic — callable from tray, shortcut, or frontend command.
pub async fn start_recording_impl(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();

    if state.is_recording.load(Ordering::Relaxed) {
        return Err("Already recording".to_string());
    }

    // Clear all track buffers
    for track in state.tracks.values() {
        let mut buffer = track.buffer.lock().await;
        buffer.clear();
    }

    let output_folder = state.output_folder.lock().await.clone();
    let session_id = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();

    // Build tracks info: only include tracks that have a device (or use default)
    let mut session_tracks = HashMap::new();
    let mut active_tracks = HashMap::new();

    for (name, track) in &state.tracks {
        let device = track.device.lock().await;
        let is_enabled = track.enabled.load(Ordering::Relaxed);
        let has_device = device.is_some() || track.use_default_when_none;

        if has_device && is_enabled {
            let sample_rate = *track.sample_rate.lock().await;
            let device_name = device
                .as_ref()
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "Default".to_string());

            session_tracks.insert(
                name.to_string(),
                crate::session::TrackInfo {
                    sample_rate,
                    device_name,
                },
            );

            active_tracks.insert(
                *name,
                ActiveTrackSession {
                    chunk_index: 0,
                    accumulated_transcript: String::new(),
                },
            );
        }
    }

    let session_dir =
        crate::session::create_session(&output_folder, &session_id, session_tracks)
            .map_err(|e| e.to_string())?;

    eprintln!(
        "[session] Created session: {} at {:?} with {} track(s)",
        session_id,
        session_dir,
        active_tracks.len()
    );

    {
        let mut active = state.active_session.lock().await;
        *active = Some(ActiveSession {
            session_dir,
            tracks: active_tracks,
        });
    }

    state.is_recording.store(true, Ordering::Relaxed);
    state.app_status.store(STATUS_RECORDING, Ordering::Relaxed);
    *state.recording_started_at.lock().unwrap() = Some(std::time::Instant::now());
    crate::tray::update_tray_menu(app);

    // The recording popover is the active-session surface. Show it
    // so the user sees the timer + partial transcripts immediately,
    // even when the session was kicked off from the tray or shortcut.
    crate::windows::show_recording(app);

    // Notify frontend (if window is open) so it can sync UI
    let _ = app.emit("recording-started", ());

    Ok(())
}

#[tauri::command]
pub async fn start_recording(app: AppHandle) -> Result<(), String> {
    start_recording_impl(&app).await
}

/// Core recording stop logic — callable from tray, shortcut, or frontend command.
pub async fn stop_recording_impl(app: &AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();

    if !state.is_recording.load(Ordering::Relaxed) {
        return Err("Not recording".to_string());
    }

    state.is_recording.store(false, Ordering::Relaxed);
    state
        .app_status
        .store(STATUS_TRANSCRIBING, Ordering::Relaxed);
    let duration_seconds: Option<u64> = state
        .recording_started_at
        .lock()
        .ok()
        .and_then(|guard| guard.map(|t| t.elapsed().as_secs()));
    *state.recording_started_at.lock().unwrap() = None;
    crate::tray::update_tray_menu(app);

    // Small delay to let final audio chunks arrive
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let _ = app.emit("transcription-status", "processing");

    // Process final chunks for all active tracks
    let track_names: Vec<TrackName> = {
        let session_guard = state.active_session.lock().await;
        session_guard
            .as_ref()
            .map(|s| s.tracks.keys().copied().collect())
            .unwrap_or_default()
    };

    for track_name in &track_names {
        match crate::pipeline::process_track_chunk(app, *track_name).await {
            Ok(_) => {}
            Err(e) => {
                eprintln!(
                    "[session] Final chunk {}: {} (may be empty)",
                    track_name, e
                );
            }
        }
    }

    // Gather transcripts per track
    let (transcripts, session_dir) = {
        let mut session_guard = state.active_session.lock().await;
        let session = session_guard.take().ok_or("No active session")?;
        let transcripts: HashMap<TrackName, String> = session
            .tracks
            .into_iter()
            .map(|(name, ts)| (name, ts.accumulated_transcript))
            .collect();
        (transcripts, session.session_dir)
    };

    let full_transcript = build_transcript(&transcripts);

    if full_transcript.is_empty() {
        state.app_status.store(STATUS_IDLE, Ordering::Relaxed);
        crate::tray::update_tray_menu(app);
        crate::windows::hide_recording(app);
        return Err("No audio recorded".to_string());
    }

    // Mark session as completed
    crate::session::complete_session(&session_dir).map_err(|e| e.to_string())?;

    eprintln!(
        "[session] Completed, transcript length: {} chars",
        full_transcript.len()
    );

    // Save final markdown. Pull the meta off the persisted manifest
    // so device names + chunk count match exactly what the recovery
    // path would write for the same session.
    let output_folder = state.output_folder.lock().await.clone();
    let meta = match crate::session::read_manifest(&session_dir) {
        Ok(manifest) => build_meta_from_manifest(&manifest, duration_seconds),
        Err(e) => {
            eprintln!("[session] read_manifest failed at stop: {}", e);
            TranscriptMeta {
                duration_seconds,
                ..Default::default()
            }
        }
    };
    save_markdown(&output_folder, &full_transcript, &meta).map_err(|e| e.to_string())?;

    // Copy to clipboard
    copy_to_clipboard(&full_transcript);

    let _ = app.emit("transcription-complete", &full_transcript);

    // Auto-hide the recording popover when the session wraps up and
    // show the Main window so the user sees the new entry in the
    // history pane without an extra click.
    crate::windows::hide_recording(app);
    crate::windows::show_main(app);

    state.app_status.store(STATUS_IDLE, Ordering::Relaxed);
    crate::tray::update_tray_menu(app);

    Ok(full_transcript)
}

/// Toggle recording on/off — used by tray menu and global shortcut.
pub async fn toggle_recording_impl(app: &AppHandle) {
    let state = app.state::<AppState>();
    let status = state.app_status.load(Ordering::Relaxed);
    if status == STATUS_TRANSCRIBING {
        return;
    }
    if state.is_recording.load(Ordering::Relaxed) {
        let _ = stop_recording_impl(app).await;
    } else {
        let _ = start_recording_impl(app).await;
    }
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> Result<String, String> {
    stop_recording_impl(&app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_transcript_empty_map_returns_empty_string() {
        let transcripts: HashMap<TrackName, String> = HashMap::new();
        assert_eq!(build_transcript(&transcripts), "");
    }

    #[test]
    fn build_transcript_single_track_returns_plain_text() {
        let mut t = HashMap::new();
        t.insert(TrackName::Microphone, "hello world".to_string());
        assert_eq!(build_transcript(&t), "hello world");
    }

    #[test]
    fn build_transcript_only_empty_track_returns_empty_string() {
        let mut t = HashMap::new();
        t.insert(TrackName::Microphone, String::new());
        assert_eq!(build_transcript(&t), "");
    }

    #[test]
    fn build_transcript_drops_empty_tracks_when_aggregating() {
        // Microphone empty, system non-empty -> single-track plain
        // text, no header.
        let mut t = HashMap::new();
        t.insert(TrackName::Microphone, String::new());
        t.insert(TrackName::System, "system text".to_string());
        assert_eq!(build_transcript(&t), "system text");
    }

    #[test]
    fn build_transcript_two_tracks_renders_markdown_headers_in_fixed_order() {
        let mut t = HashMap::new();
        // Insert system first to make sure the order in the output is
        // driven by the [Microphone, System] ordering, not insertion
        // order.
        t.insert(TrackName::System, "system text".to_string());
        t.insert(TrackName::Microphone, "mic text".to_string());

        let out = build_transcript(&t);
        let expected = format!(
            "## {}\n\nmic text\n\n## {}\n\nsystem text",
            TrackName::Microphone.display_label(),
            TrackName::System.display_label(),
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn build_transcript_two_tracks_one_empty_does_not_render_headers() {
        // Per the existing logic, a single non-empty track skips the
        // headers entirely — even when a sibling track was present in
        // the map but came back blank.
        let mut t = HashMap::new();
        t.insert(TrackName::System, "".to_string());
        t.insert(TrackName::Microphone, "mic only".to_string());
        assert_eq!(build_transcript(&t), "mic only");
    }

    #[test]
    fn build_frontmatter_with_full_meta_emits_every_row() {
        let meta = TranscriptMeta {
            duration_seconds: Some(155),
            mic_device: Some("MacBook Pro Microphone".to_string()),
            sys_device: Some("ScreenCaptureKit".to_string()),
            chunks: Some(12),
        };
        let fm = build_frontmatter("2026-05-22 16:08:26", &meta);
        assert!(fm.contains("# Voice transcription"));
        assert!(fm.contains("**Date:** 2026-05-22 16:08:26"));
        assert!(fm.contains("**Duration:** 02:35"));
        assert!(fm.contains("**Language:** Portuguese (BR)"));
        assert!(fm.contains("**Microphone:** MacBook Pro Microphone"));
        assert!(fm.contains("**System audio:** ScreenCaptureKit"));
        assert!(fm.contains("**Chunks:** 12"));
    }

    #[test]
    fn build_frontmatter_omits_rows_for_none_fields() {
        // The "empty meta" case mirrors the recovery flow: the
        // session crashed before stop, so we have no duration and
        // perhaps no device info either. The Detail pane is supposed
        // to hide rows whose field is absent, so the file we save
        // had better leave them out instead of writing a blank row.
        let meta = TranscriptMeta::default();
        let fm = build_frontmatter("2026-05-22 16:08:26", &meta);
        assert!(fm.contains("**Date:** 2026-05-22 16:08:26"));
        assert!(fm.contains("**Language:** Portuguese (BR)"));
        assert!(!fm.contains("**Duration:"));
        assert!(!fm.contains("**Microphone:"));
        assert!(!fm.contains("**System audio:"));
        assert!(!fm.contains("**Chunks:"));
    }

    #[test]
    fn build_frontmatter_duration_formats_hours_when_long() {
        let meta = TranscriptMeta {
            duration_seconds: Some(3661),
            ..Default::default()
        };
        let fm = build_frontmatter("now", &meta);
        assert!(fm.contains("**Duration:** 01:01:01"));
    }

    #[test]
    fn build_meta_from_manifest_pulls_devices_and_chunk_count() {
        use crate::session::{SessionChunk, SessionManifest, SessionStatus, TrackInfo};
        let mut tracks = HashMap::new();
        tracks.insert(
            TrackName::Microphone.to_string(),
            TrackInfo {
                sample_rate: 48000,
                device_name: "AirPods".to_string(),
            },
        );
        tracks.insert(
            TrackName::System.to_string(),
            TrackInfo {
                sample_rate: 48000,
                device_name: "ScreenCaptureKit".to_string(),
            },
        );
        let manifest = SessionManifest {
            session_id: "x".to_string(),
            started_at: "now".to_string(),
            tracks,
            status: SessionStatus::Completed,
            chunks: vec![
                SessionChunk {
                    filename: "mic_000.wav".to_string(),
                    track: "microphone".to_string(),
                    transcript: Some("a".to_string()),
                },
                SessionChunk {
                    filename: "sys_000.wav".to_string(),
                    track: "system".to_string(),
                    transcript: Some("b".to_string()),
                },
            ],
        };
        let meta = build_meta_from_manifest(&manifest, Some(45));
        assert_eq!(meta.duration_seconds, Some(45));
        assert_eq!(meta.mic_device.as_deref(), Some("AirPods"));
        assert_eq!(meta.sys_device.as_deref(), Some("ScreenCaptureKit"));
        assert_eq!(meta.chunks, Some(2));
    }

    #[test]
    fn build_meta_from_manifest_omits_chunks_when_empty() {
        use crate::session::{SessionManifest, SessionStatus};
        let manifest = SessionManifest {
            session_id: "x".to_string(),
            started_at: "now".to_string(),
            tracks: HashMap::new(),
            status: SessionStatus::Completed,
            chunks: vec![],
        };
        let meta = build_meta_from_manifest(&manifest, None);
        assert!(meta.duration_seconds.is_none());
        assert!(meta.mic_device.is_none());
        assert!(meta.sys_device.is_none());
        assert!(meta.chunks.is_none());
    }
}
