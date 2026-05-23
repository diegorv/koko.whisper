use crate::audio::SelectedDevice;
use crate::config::AppConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32};
use std::sync::Arc;

pub const STATUS_IDLE: u8 = 0;
pub const STATUS_RECORDING: u8 = 1;
pub const STATUS_TRANSCRIBING: u8 = 2;
use tokio::sync::{watch, Mutex};
use whisper_rs::WhisperContext;

/// Identifies an audio track by type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackName {
    Microphone,
    System,
}

impl std::fmt::Display for TrackName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackName::Microphone => write!(f, "microphone"),
            TrackName::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for TrackName {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "microphone" => Ok(TrackName::Microphone),
            "system" => Ok(TrackName::System),
            _ => Err(format!("Unknown track: {}", s)),
        }
    }
}

impl TrackName {
    /// Human-readable label for transcript headers.
    pub fn display_label(&self) -> &'static str {
        match self {
            TrackName::Microphone => "Microphone",
            TrackName::System => "System audio",
        }
    }
}

/// Lifecycle state of the Whisper model. The model boot sequence
/// (download → init) runs in `setup()` so windows can show a splash
/// while it completes. Frontend reads the snapshot via
/// `get_model_status` and subscribes to `model-status` events for
/// transitions.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ModelStatus {
    Unchecked,
    Downloading { progress: f64 },
    Ready,
    Error { message: String },
}

/// State for a single audio track (microphone, system audio, etc.)
pub struct TrackState {
    pub buffer: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: Arc<Mutex<u32>>,
    pub device: Arc<Mutex<Option<SelectedDevice>>>,
    pub change_tx: watch::Sender<Option<SelectedDevice>>,
    pub change_rx: watch::Receiver<Option<SelectedDevice>>,
    /// If true, use system default input device when device is None.
    /// If false, don't capture when device is None (e.g. system audio disabled).
    pub use_default_when_none: bool,
    /// Peak audio level (f32 stored as bits) for VU metering.
    pub peak_level: Arc<AtomicU32>,
    /// Whether this track is enabled for capture/recording.
    pub enabled: Arc<AtomicBool>,
}

impl TrackState {
    pub fn new(use_default_when_none: bool, enabled: bool) -> Self {
        let (change_tx, change_rx) = watch::channel(None);
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: Arc::new(Mutex::new(48000)),
            device: Arc::new(Mutex::new(None)),
            change_tx,
            change_rx,
            use_default_when_none,
            peak_level: Arc::new(AtomicU32::new(0)),
            enabled: Arc::new(AtomicBool::new(enabled)),
        }
    }
}

/// Per-track state within an active recording session
pub struct ActiveTrackSession {
    pub chunk_index: u32,
    pub accumulated_transcript: String,
}

/// Tracks the current active recording session at runtime
pub struct ActiveSession {
    pub session_dir: PathBuf,
    pub tracks: HashMap<TrackName, ActiveTrackSession>,
}

pub struct AppState {
    pub whisper_context: Arc<Mutex<Option<Arc<WhisperContext>>>>,
    pub is_recording: Arc<AtomicBool>,
    pub output_folder: Arc<Mutex<PathBuf>>,
    pub tracks: HashMap<TrackName, TrackState>,
    /// Active recording session (None when not recording)
    pub active_session: Arc<Mutex<Option<ActiveSession>>>,
    /// UI status for the tray menu (STATUS_IDLE, STATUS_RECORDING, STATUS_TRANSCRIBING)
    pub app_status: Arc<AtomicU8>,
    /// When the current recording started (for timer display)
    pub recording_started_at: Arc<std::sync::Mutex<Option<std::time::Instant>>>,
    /// Snapshot of the Whisper model lifecycle. Set by the boot task
    /// in `setup()`. Frontend windows read it via `get_model_status`
    /// and subscribe to `model-status` events for transitions.
    pub model_status: Arc<Mutex<ModelStatus>>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let default_output = dirs::document_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("KokoNotesWhisper");

        let output_folder = config
            .output_folder
            .map(PathBuf::from)
            .unwrap_or(default_output);

        let mut tracks = HashMap::new();
        // Microphone: always captures (uses default device when None)
        let mic_track = TrackState::new(true, config.mic_enabled);
        *mic_track.device.blocking_lock() = config.mic_device.clone();
        if config.mic_device.is_some() {
            let _ = mic_track.change_tx.send(config.mic_device);
        }
        tracks.insert(TrackName::Microphone, mic_track);

        // System audio: only captures when explicitly configured
        let sys_track = TrackState::new(false, config.sys_enabled);
        *sys_track.device.blocking_lock() = config.sys_device.clone();
        if config.sys_device.is_some() {
            let _ = sys_track.change_tx.send(config.sys_device);
        }
        tracks.insert(TrackName::System, sys_track);

        Self {
            whisper_context: Arc::new(Mutex::new(None)),
            is_recording: Arc::new(AtomicBool::new(false)),
            output_folder: Arc::new(Mutex::new(output_folder)),
            tracks,
            active_session: Arc::new(Mutex::new(None)),
            app_status: Arc::new(AtomicU8::new(STATUS_IDLE)),
            recording_started_at: Arc::new(std::sync::Mutex::new(None)),
            model_status: Arc::new(Mutex::new(ModelStatus::Unchecked)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn track_name_display_uses_lowercase_words() {
        assert_eq!(TrackName::Microphone.to_string(), "microphone");
        assert_eq!(TrackName::System.to_string(), "system");
    }

    #[test]
    fn track_name_from_str_parses_lowercase_words() {
        assert_eq!(TrackName::from_str("microphone").unwrap(), TrackName::Microphone);
        assert_eq!(TrackName::from_str("system").unwrap(), TrackName::System);
    }

    #[test]
    fn track_name_from_str_rejects_unknown() {
        assert!(TrackName::from_str("speaker").is_err());
        assert!(TrackName::from_str("").is_err());
        assert!(TrackName::from_str("Microphone").is_err()); // case-sensitive
    }

    #[test]
    fn track_name_display_and_from_str_round_trip() {
        for track in [TrackName::Microphone, TrackName::System] {
            let s = track.to_string();
            assert_eq!(TrackName::from_str(&s).unwrap(), track);
        }
    }

    #[test]
    fn track_name_display_label_pinned_per_variant() {
        // Pin the English labels rendered in transcript headers. The
        // PT-BR labels were dropped in ui-02 (commit replaces the
        // app's UI strings with English). The aggregate markdown
        // format documented in CONTEXT.md ("Transcription") still
        // pairs one heading per Track, just under the new strings.
        assert_eq!(TrackName::Microphone.display_label(), "Microphone");
        assert_eq!(TrackName::System.display_label(), "System audio");
    }

    #[test]
    fn track_name_serde_uses_lowercase_words() {
        // The persisted session manifest stores track names as strings
        // (see session::SessionChunk.track). Pin the wire format so a
        // future rename of the enum variants does not invalidate
        // existing manifests on disk.
        let json = serde_json::to_string(&TrackName::Microphone).unwrap();
        assert_eq!(json, "\"microphone\"");

        let parsed: TrackName = serde_json::from_str("\"system\"").unwrap();
        assert_eq!(parsed, TrackName::System);
    }

    #[test]
    fn status_constants_have_distinct_values_starting_at_idle_zero() {
        // The tray + UI compare these constants via plain integer
        // equality on an AtomicU8. Pin their values so a reorder does
        // not silently move "recording" to a different bit pattern.
        assert_eq!(STATUS_IDLE, 0);
        assert_eq!(STATUS_RECORDING, 1);
        assert_eq!(STATUS_TRANSCRIBING, 2);

        // Distinctness invariant.
        let codes = [STATUS_IDLE, STATUS_RECORDING, STATUS_TRANSCRIBING];
        for (i, a) in codes.iter().enumerate() {
            for b in &codes[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
