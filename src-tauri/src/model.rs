use anyhow::Result;
use futures::StreamExt;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin";
const MODEL_FILENAME: &str = "ggml-large-v3-turbo-q5_0.bin";

pub fn get_models_dir() -> Result<PathBuf> {
    let data_dir =
        dirs::data_dir().ok_or_else(|| anyhow::anyhow!("Cannot find Application Support dir"))?;
    let models_dir = data_dir.join("koko-notes-whisper").join("models");
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
