//! Transcription history Tauri commands. Lists the markdown files in
//! the user's output folder, surfacing both a list-row preview and
//! the parsed frontmatter so the Main two-pane history pane (ADR-0002
//! §4) can render meta rows without re-reading every file.
//!
//! Frontmatter parsing tolerates missing fields. New sessions (post
//! ui-02) write the full Date / Duration / Language / Microphone /
//! System audio / Chunks block; legacy `.md` files written before
//! the ui-02 sweep only have Date + Language (or the PT-BR labels
//! "Data:" / "Idioma:" — also accepted). The Detail pane omits any
//! field that comes back `None`.

use crate::state::AppState;
use tauri::State;

#[derive(serde::Serialize, Clone)]
pub struct TranscriptionEntry {
    pub filename: String,
    pub path: String,
    pub preview: String,
    pub date: Option<String>,
    pub duration_seconds: Option<u32>,
    pub language: Option<String>,
    pub mic_device: Option<String>,
    pub sys_device: Option<String>,
    pub chunks: Option<u32>,
}

/// Pure. Split a markdown file into (frontmatter, body) at the first
/// standalone `---` separator line. If there is no separator the
/// body is the whole content and the frontmatter is empty.
fn split_frontmatter(content: &str) -> (&str, &str) {
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        let stripped = line.trim_end_matches(['\n', '\r']);
        if stripped == "---" {
            let body_start = offset + line.len();
            return (content[..offset].trim_end(), &content[body_start..]);
        }
        offset += line.len();
    }
    ("", content)
}

/// Pure. First N chars of the body, trimmed.
fn extract_preview(body: &str) -> String {
    body.trim().chars().take(150).collect::<String>()
}

/// Pure. Strip a leading "**Label:**" prefix from a frontmatter line
/// and return the trimmed value. Returns `None` when the line does
/// not match.
fn match_field<'a>(line: &'a str, labels: &[&str]) -> Option<&'a str> {
    let trimmed = line.trim();
    for label in labels {
        if let Some(rest) = trimmed.strip_prefix(*label) {
            return Some(rest.trim());
        }
    }
    None
}

/// Pure. Parse "01:23" / "1:23:45" / "00:05" into seconds.
fn parse_duration(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.split(':').collect();
    let nums: Result<Vec<u32>, _> = parts.iter().map(|p| p.trim().parse::<u32>()).collect();
    let nums = nums.ok()?;
    match nums.len() {
        2 => Some(nums[0] * 60 + nums[1]),
        3 => Some(nums[0] * 3600 + nums[1] * 60 + nums[2]),
        _ => None,
    }
}

#[derive(Default)]
struct ParsedFrontmatter {
    date: Option<String>,
    duration_seconds: Option<u32>,
    language: Option<String>,
    mic_device: Option<String>,
    sys_device: Option<String>,
    chunks: Option<u32>,
}

/// Pure. Pull each known meta row out of the frontmatter block. The
/// recognised labels include both the EN form (post ui-02) and the
/// legacy PT-BR labels so old files still render with their Date and
/// Language rows populated.
fn parse_frontmatter(text: &str) -> ParsedFrontmatter {
    let mut out = ParsedFrontmatter::default();
    for line in text.lines() {
        if let Some(v) = match_field(line, &["**Date:**", "**Data:**"]) {
            out.date = Some(v.to_string());
        } else if let Some(v) = match_field(line, &["**Duration:**", "**Duracao:**"]) {
            out.duration_seconds = parse_duration(v);
        } else if let Some(v) = match_field(line, &["**Language:**", "**Idioma:**"]) {
            out.language = Some(v.to_string());
        } else if let Some(v) = match_field(line, &["**Microphone:**", "**Microfone:**"]) {
            out.mic_device = Some(v.to_string());
        } else if let Some(v) =
            match_field(line, &["**System audio:**", "**Audio do sistema:**", "**Áudio do sistema:**"])
        {
            out.sys_device = Some(v.to_string());
        } else if let Some(v) = match_field(line, &["**Chunks:**", "**Trechos:**"]) {
            out.chunks = v.trim().parse::<u32>().ok();
        }
    }
    out
}

#[tauri::command]
pub async fn get_transcriptions(
    state: State<'_, AppState>,
) -> Result<Vec<TranscriptionEntry>, String> {
    let output_folder = state.output_folder.lock().await.clone();
    let mut entries = Vec::new();

    if !output_folder.exists() {
        return Ok(entries);
    }

    let mut paths: Vec<_> = std::fs::read_dir(&output_folder)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        .collect();
    // Newest first. Filenames are `YYYY-MM-DD_HH-MM-SS.md` so a
    // lexicographic reverse sort is also chronological.
    paths.sort_by_key(|e| std::cmp::Reverse(e.file_name()));

    for entry in paths {
        let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
        let (frontmatter, body) = split_frontmatter(&content);
        let parsed = parse_frontmatter(frontmatter);
        entries.push(TranscriptionEntry {
            filename: entry.file_name().to_string_lossy().to_string(),
            path: entry.path().to_string_lossy().to_string(),
            preview: extract_preview(body),
            date: parsed.date,
            duration_seconds: parsed.duration_seconds,
            language: parsed.language,
            mic_device: parsed.mic_device,
            sys_device: parsed.sys_device,
            chunks: parsed.chunks,
        });
    }

    Ok(entries)
}

/// Pure. Validate a transcription file path against the configured
/// output folder root. Returns the canonical target path on success,
/// a human-readable error string on rejection. Centralised so the
/// `get_transcription_body` and `delete_transcription` paths apply
/// the same rules — extension must be `.md`, canonical path must
/// resolve inside the folder root.
fn validate_transcription_path(
    path: &str,
    output_folder: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let target = std::path::PathBuf::from(path);

    if target.extension().map_or(true, |ext| ext != "md") {
        return Err("Path must be a .md file".to_string());
    }
    let canonical = std::fs::canonicalize(&target).map_err(|e| e.to_string())?;
    let folder_canonical = std::fs::canonicalize(output_folder).map_err(|e| e.to_string())?;
    if !canonical.starts_with(&folder_canonical) {
        return Err("Path is outside the transcription folder".to_string());
    }
    Ok(canonical)
}

/// Returns the full body of a single transcription file. Path is
/// validated to live inside the configured output folder and to end
/// in `.md` so the command cannot be used as an arbitrary file
/// reader.
#[tauri::command]
pub async fn get_transcription_body(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let output_folder = state.output_folder.lock().await.clone();
    let canonical = validate_transcription_path(&path, &output_folder)?;

    let content = std::fs::read_to_string(&canonical).map_err(|e| e.to_string())?;
    let (_frontmatter, body) = split_frontmatter(&content);
    Ok(body.trim_start().to_string())
}

/// Permanently delete a transcription `.md` from the output folder.
/// The path is canonicalised and validated to live inside the
/// configured folder so the command cannot be used to remove
/// arbitrary files. Returns the canonicalised path on success.
#[tauri::command]
pub async fn delete_transcription(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let output_folder = state.output_folder.lock().await.clone();
    let canonical = validate_transcription_path(&path, &output_folder)?;

    std::fs::remove_file(&canonical).map_err(|e| e.to_string())?;
    Ok(canonical.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_frontmatter_extracts_body_after_separator() {
        let md = "# Voice transcription\n\n**Date:** 2026-05-22 16:08:26\n**Language:** Portuguese (BR)\n\n---\n\nHello world\n";
        let (fm, body) = split_frontmatter(md);
        assert!(fm.contains("**Date:**"));
        assert_eq!(body.trim(), "Hello world");
    }

    #[test]
    fn split_frontmatter_returns_full_content_when_no_separator() {
        let md = "Just a plain body, no separator.";
        let (fm, body) = split_frontmatter(md);
        assert_eq!(fm, "");
        assert_eq!(body, "Just a plain body, no separator.");
    }

    #[test]
    fn parse_frontmatter_reads_full_meta_block() {
        let fm = "# Voice transcription\n\n**Date:** 2026-05-22 16:08:26\n**Duration:** 02:35\n**Language:** Portuguese (BR)\n**Microphone:** MacBook Pro Microphone\n**System audio:** ScreenCaptureKit\n**Chunks:** 12";
        let p = parse_frontmatter(fm);
        assert_eq!(p.date.as_deref(), Some("2026-05-22 16:08:26"));
        assert_eq!(p.duration_seconds, Some(155));
        assert_eq!(p.language.as_deref(), Some("Portuguese (BR)"));
        assert_eq!(p.mic_device.as_deref(), Some("MacBook Pro Microphone"));
        assert_eq!(p.sys_device.as_deref(), Some("ScreenCaptureKit"));
        assert_eq!(p.chunks, Some(12));
    }

    #[test]
    fn parse_frontmatter_accepts_legacy_pt_br_labels() {
        // Legacy `.md` files saved before the ui-02 sweep used PT-BR
        // labels. The parser keeps them so Date + Language still
        // populate on those rows.
        let fm = "# Transcricao de Voz\n\n**Data:** 2025-12-01 09:00:00\n**Idioma:** Portugues (BR)\n**Microfone:** AirPods\n**Trechos:** 4";
        let p = parse_frontmatter(fm);
        assert_eq!(p.date.as_deref(), Some("2025-12-01 09:00:00"));
        assert_eq!(p.language.as_deref(), Some("Portugues (BR)"));
        assert_eq!(p.mic_device.as_deref(), Some("AirPods"));
        assert_eq!(p.chunks, Some(4));
        // Duration absent — `None`, not `Some(0)`.
        assert!(p.duration_seconds.is_none());
        assert!(p.sys_device.is_none());
    }

    #[test]
    fn parse_frontmatter_handles_completely_empty_meta() {
        let p = parse_frontmatter("");
        assert!(p.date.is_none());
        assert!(p.duration_seconds.is_none());
        assert!(p.language.is_none());
        assert!(p.mic_device.is_none());
        assert!(p.sys_device.is_none());
        assert!(p.chunks.is_none());
    }

    #[test]
    fn parse_duration_two_part_returns_seconds() {
        assert_eq!(parse_duration("02:35"), Some(155));
        assert_eq!(parse_duration("00:00"), Some(0));
        assert_eq!(parse_duration("59:59"), Some(3599));
    }

    #[test]
    fn parse_duration_three_part_returns_seconds() {
        assert_eq!(parse_duration("01:00:00"), Some(3600));
        assert_eq!(parse_duration("01:01:01"), Some(3661));
        assert_eq!(parse_duration("10:00:00"), Some(36000));
    }

    #[test]
    fn parse_duration_rejects_garbage() {
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration("12"), None);
        assert_eq!(parse_duration(":"), None);
        assert_eq!(parse_duration("1:2:3:4"), None);
    }

    #[test]
    fn extract_preview_truncates_at_150_chars() {
        let body = "a".repeat(200);
        let preview = extract_preview(&body);
        assert_eq!(preview.chars().count(), 150);
        assert!(preview.chars().all(|c| c == 'a'));
    }

    #[test]
    fn extract_preview_counts_chars_not_bytes_for_multibyte_input() {
        let body = "á".repeat(60);
        let preview = extract_preview(&body);
        assert_eq!(preview.chars().count(), 60);
    }

    fn validation_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "koko_whisper_validation_{}_{}",
            name,
            std::process::id()
        ))
    }

    #[test]
    fn validate_transcription_path_accepts_md_under_folder() {
        let folder = validation_dir("accept");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("2026-05-22_16-00-00.md");
        std::fs::write(&file, "stub").unwrap();

        let result = validate_transcription_path(file.to_str().unwrap(), &folder);
        assert!(result.is_ok());

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn validate_transcription_path_rejects_non_md_extension() {
        let folder = validation_dir("ext");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("notes.txt");
        std::fs::write(&file, "stub").unwrap();

        let err = validate_transcription_path(file.to_str().unwrap(), &folder)
            .expect_err("non-.md path must be rejected");
        assert!(err.contains(".md"));

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn validate_transcription_path_rejects_path_outside_folder() {
        let folder = validation_dir("outside_folder");
        let other = validation_dir("outside_other");
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::remove_dir_all(&other);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let file = other.join("rogue.md");
        std::fs::write(&file, "stub").unwrap();

        let err = validate_transcription_path(file.to_str().unwrap(), &folder)
            .expect_err("path outside folder must be rejected");
        assert!(err.contains("outside"));

        std::fs::remove_dir_all(&folder).unwrap();
        std::fs::remove_dir_all(&other).unwrap();
    }
}
