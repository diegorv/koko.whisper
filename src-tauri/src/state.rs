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
            TrackName::Microphone => "Eu (Microfone)",
            TrackName::System => "Participante (Audio do Sistema)",
        }
    }
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
        }
    }
}
