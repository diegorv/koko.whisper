use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Recording,
    Completed,
    Recovered,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TrackInfo {
    pub sample_rate: u32,
    pub device_name: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SessionChunk {
    pub filename: String,
    pub track: String,
    pub transcript: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SessionManifest {
    pub session_id: String,
    pub started_at: String,
    pub tracks: HashMap<String, TrackInfo>,
    pub status: SessionStatus,
    pub chunks: Vec<SessionChunk>,
}

/// Get the sessions directory within the output folder
pub fn sessions_dir(output_folder: &Path) -> PathBuf {
    output_folder.join("sessions")
}

/// Create a new session directory and write initial manifest
pub fn create_session(
    output_folder: &Path,
    session_id: &str,
    tracks: HashMap<String, TrackInfo>,
) -> Result<PathBuf> {
    let dir = sessions_dir(output_folder).join(session_id);
    std::fs::create_dir_all(&dir)?;

    let manifest = SessionManifest {
        session_id: session_id.to_string(),
        started_at: chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%z")
            .to_string(),
        tracks,
        status: SessionStatus::Recording,
        chunks: Vec::new(),
    };

    write_manifest(&dir, &manifest)?;
    Ok(dir)
}

/// Read the session manifest from a session directory
pub fn read_manifest(session_dir: &Path) -> Result<SessionManifest> {
    let path = session_dir.join("session.json");
    let content = std::fs::read_to_string(&path)?;
    let manifest: SessionManifest = serde_json::from_str(&content)?;
    Ok(manifest)
}

/// Write/update the session manifest
pub fn write_manifest(session_dir: &Path, manifest: &SessionManifest) -> Result<()> {
    let path = session_dir.join("session.json");
    let content = serde_json::to_string_pretty(manifest)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Add a chunk entry to the manifest (transcript initially None)
pub fn add_chunk_to_manifest(session_dir: &Path, filename: &str, track: &str) -> Result<()> {
    let mut manifest = read_manifest(session_dir)?;
    manifest.chunks.push(SessionChunk {
        filename: filename.to_string(),
        track: track.to_string(),
        transcript: None,
    });
    write_manifest(session_dir, &manifest)?;
    Ok(())
}

/// Update the transcript for a specific chunk
pub fn update_chunk_transcript(
    session_dir: &Path,
    chunk_filename: &str,
    transcript: &str,
) -> Result<()> {
    let mut manifest = read_manifest(session_dir)?;
    if let Some(chunk) = manifest
        .chunks
        .iter_mut()
        .find(|c| c.filename == chunk_filename)
    {
        chunk.transcript = Some(transcript.to_string());
    }
    write_manifest(session_dir, &manifest)?;
    Ok(())
}

/// Mark session as completed
pub fn complete_session(session_dir: &Path) -> Result<()> {
    let mut manifest = read_manifest(session_dir)?;
    manifest.status = SessionStatus::Completed;
    write_manifest(session_dir, &manifest)?;
    Ok(())
}

/// Find incomplete sessions (status == Recording) for crash recovery
pub fn find_incomplete_sessions(output_folder: &Path) -> Result<Vec<PathBuf>> {
    let sessions = sessions_dir(output_folder);
    if !sessions.exists() {
        return Ok(Vec::new());
    }

    let mut incomplete = Vec::new();
    for entry in std::fs::read_dir(&sessions)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let manifest_path = entry.path().join("session.json");
            if manifest_path.exists() {
                if let Ok(manifest) = read_manifest(&entry.path()) {
                    if manifest.status == SessionStatus::Recording {
                        incomplete.push(entry.path());
                    }
                }
            }
        }
    }
    Ok(incomplete)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("noted_whisper_test_{}_{}", name, std::process::id()))
    }

    #[test]
    fn test_session_lifecycle() {
        let tmp = test_dir("lifecycle");
        let _ = std::fs::remove_dir_all(&tmp);

        let mut tracks = HashMap::new();
        tracks.insert(
            "microphone".to_string(),
            TrackInfo {
                sample_rate: 48000,
                device_name: "Test Mic".to_string(),
            },
        );

        // Create session
        let session_dir = create_session(&tmp, "test-session", tracks).unwrap();
        assert!(session_dir.exists());

        // Read manifest
        let manifest = read_manifest(&session_dir).unwrap();
        assert_eq!(manifest.session_id, "test-session");
        assert_eq!(manifest.status, SessionStatus::Recording);
        assert!(manifest.chunks.is_empty());

        // Add chunk
        add_chunk_to_manifest(&session_dir, "mic_000.wav", "microphone").unwrap();
        let manifest = read_manifest(&session_dir).unwrap();
        assert_eq!(manifest.chunks.len(), 1);
        assert_eq!(manifest.chunks[0].filename, "mic_000.wav");
        assert_eq!(manifest.chunks[0].track, "microphone");
        assert!(manifest.chunks[0].transcript.is_none());

        // Update transcript
        update_chunk_transcript(&session_dir, "mic_000.wav", "Hello world").unwrap();
        let manifest = read_manifest(&session_dir).unwrap();
        assert_eq!(
            manifest.chunks[0].transcript.as_deref(),
            Some("Hello world")
        );

        // Complete session
        complete_session(&session_dir).unwrap();
        let manifest = read_manifest(&session_dir).unwrap();
        assert_eq!(manifest.status, SessionStatus::Completed);

        // Find incomplete (should be empty now)
        let incomplete = find_incomplete_sessions(&tmp).unwrap();
        assert!(incomplete.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_find_incomplete_sessions() {
        let tmp = test_dir("incomplete");
        let _ = std::fs::remove_dir_all(&tmp);

        let mut tracks = HashMap::new();
        tracks.insert(
            "microphone".to_string(),
            TrackInfo {
                sample_rate: 48000,
                device_name: "Test".to_string(),
            },
        );

        // Create one incomplete session
        let _ = create_session(&tmp, "incomplete-1", tracks.clone()).unwrap();

        // Create one completed session
        let completed_dir = create_session(&tmp, "completed-1", tracks).unwrap();
        complete_session(&completed_dir).unwrap();

        let incomplete = find_incomplete_sessions(&tmp).unwrap();
        assert_eq!(incomplete.len(), 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_session_status_serialization() {
        let status = SessionStatus::Recording;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"recording\"");

        let loaded: SessionStatus = serde_json::from_str("\"completed\"").unwrap();
        assert_eq!(loaded, SessionStatus::Completed);

        let loaded: SessionStatus = serde_json::from_str("\"recovered\"").unwrap();
        assert_eq!(loaded, SessionStatus::Recovered);
    }

    #[test]
    fn test_multiple_chunks_multiple_tracks() {
        let tmp = test_dir("multi_chunk");
        let _ = std::fs::remove_dir_all(&tmp);

        let mut tracks = HashMap::new();
        tracks.insert(
            "microphone".to_string(),
            TrackInfo {
                sample_rate: 48000,
                device_name: "Mic".to_string(),
            },
        );
        tracks.insert(
            "system".to_string(),
            TrackInfo {
                sample_rate: 48000,
                device_name: "System".to_string(),
            },
        );

        let session_dir = create_session(&tmp, "multi", tracks).unwrap();

        add_chunk_to_manifest(&session_dir, "microphone_000.wav", "microphone").unwrap();
        add_chunk_to_manifest(&session_dir, "system_000.wav", "system").unwrap();
        add_chunk_to_manifest(&session_dir, "microphone_001.wav", "microphone").unwrap();

        let manifest = read_manifest(&session_dir).unwrap();
        assert_eq!(manifest.chunks.len(), 3);

        update_chunk_transcript(&session_dir, "microphone_000.wav", "Hello").unwrap();
        update_chunk_transcript(&session_dir, "system_000.wav", "World").unwrap();

        let manifest = read_manifest(&session_dir).unwrap();
        assert_eq!(
            manifest.chunks[0].transcript.as_deref(),
            Some("Hello")
        );
        assert_eq!(
            manifest.chunks[1].transcript.as_deref(),
            Some("World")
        );
        assert!(manifest.chunks[2].transcript.is_none());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
