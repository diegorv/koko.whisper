//! Whisper model lifecycle. Owns the download URL, the on-disk
//! filename, the path resolution under the OS data dir, and the
//! streaming download to a temp file with atomic rename on success.
//!
//! Inference concerns (loading the model into a `WhisperContext`,
//! running params, prompt formatting) live in `transcription/`.

use anyhow::Result;
use futures::StreamExt;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin";
const MODEL_FILENAME: &str = "ggml-large-v3-turbo-q5_0.bin";

const APP_DATA_SUBDIR: &str = "koko-notes-whisper";
const MODELS_SUBDIR: &str = "models";

/// Pure path composition. Given a data-dir root (e.g. `~/Library/Application Support`),
/// returns the directory the model file is expected to live in. Does no I/O.
fn build_models_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(APP_DATA_SUBDIR).join(MODELS_SUBDIR)
}

/// Pure path composition. Given a data-dir root, returns the full path
/// the model binary is expected to live at. Does no I/O.
#[allow(dead_code)] // Currently only exercised by the test module below.
fn build_model_path(data_dir: &Path) -> PathBuf {
    build_models_dir(data_dir).join(MODEL_FILENAME)
}

pub fn get_models_dir() -> Result<PathBuf> {
    let data_dir =
        dirs::data_dir().ok_or_else(|| anyhow::anyhow!("Cannot find Application Support dir"))?;
    let models_dir = build_models_dir(&data_dir);
    std::fs::create_dir_all(&models_dir)?;
    Ok(models_dir)
}

pub fn get_model_path() -> Result<PathBuf> {
    Ok(get_models_dir()?.join(MODEL_FILENAME))
}

pub fn is_model_downloaded() -> bool {
    get_model_path().map(|p| p.exists()).unwrap_or(false)
}

pub async fn download_model(progress_callback: impl Fn(f64) + Send + 'static) -> Result<PathBuf> {
    let model_path = get_model_path()?;
    if model_path.exists() {
        progress_callback(1.0);
        return Ok(model_path);
    }

    let response = reqwest::get(MODEL_URL).await?;
    let total_size = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let tmp_path = model_path.with_extension("bin.tmp");
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if total_size > 0 {
            progress_callback(downloaded as f64 / total_size as f64);
        }
    }

    file.flush().await?;
    drop(file);

    // Atomic rename so partial downloads don't corrupt
    tokio::fs::rename(&tmp_path, &model_path).await?;

    Ok(model_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn build_models_dir_composes_app_subdir_then_models_subdir() {
        let root = PathBuf::from("/tmp/fake-data");
        assert_eq!(
            build_models_dir(&root),
            PathBuf::from("/tmp/fake-data/koko-notes-whisper/models"),
        );
    }

    #[test]
    fn build_model_path_appends_filename_to_models_dir() {
        let root = PathBuf::from("/tmp/fake-data");
        assert_eq!(
            build_model_path(&root),
            PathBuf::from("/tmp/fake-data/koko-notes-whisper/models/ggml-large-v3-turbo-q5_0.bin"),
        );
    }

    #[test]
    fn build_model_path_ends_with_filename_constant() {
        let root = PathBuf::from("/anywhere");
        let path = build_model_path(&root);
        assert_eq!(path.file_name().and_then(|n| n.to_str()), Some(MODEL_FILENAME));
    }

    #[test]
    fn model_url_points_at_filename_constant() {
        // URL and filename are independent constants. Pin the
        // invariant that the URL's last segment is what we save the
        // file as — catches the case where only one is bumped.
        let last_segment = MODEL_URL.rsplit('/').next().expect("non-empty URL");
        assert_eq!(last_segment, MODEL_FILENAME);
    }

    #[test]
    fn model_url_is_https() {
        // Downloading multi-GB ggml weights over http would be a real
        // mistake. Cheap guard against it.
        assert!(MODEL_URL.starts_with("https://"), "MODEL_URL must be https");
    }
}
