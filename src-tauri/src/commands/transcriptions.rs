//! Transcription history Tauri commands. Lists the most recent
//! markdown files in the user's output folder and surfaces them as
//! preview cards in `/transcriptions`.

use crate::state::AppState;
use tauri::State;

#[derive(serde::Serialize, Clone)]
pub struct TranscriptionEntry {
    pub filename: String,
    pub preview: String,
    pub path: String,
}

/// Pure. Given the raw markdown body produced by
/// `recording::save_markdown`, extract a short snippet for the
/// preview card: take the text after the first `---` separator
/// (i.e. drop the frontmatter-style header), trim whitespace, then
/// keep the first 150 chars. If there is no separator the whole
/// content is treated as preview.
fn extract_preview(content: &str) -> String {
    content
        .split("---")
        .nth(1)
        .unwrap_or(content)
        .trim()
        .chars()
        .take(150)
        .collect::<String>()
}

#[tauri::command]
pub async fn get_transcriptions(
    state: State<'_, AppState>,
) -> Result<Vec<TranscriptionEntry>, String> {
    let output_folder = state.output_folder.lock().await.clone();
    let mut entries = Vec::new();

    if output_folder.exists() {
        let mut paths: Vec<_> = std::fs::read_dir(&output_folder)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
            .collect();
        paths.sort_by_key(|e| std::cmp::Reverse(e.file_name()));

        for entry in paths.iter().take(20) {
            let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
            entries.push(TranscriptionEntry {
                filename: entry.file_name().to_string_lossy().to_string(),
                preview: extract_preview(&content),
                path: entry.path().to_string_lossy().to_string(),
            });
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_preview_drops_header_above_first_dashes() {
        // The markdown produced by `recording::save_markdown` has a
        // header block above a `---` separator; preview should be the
        // body below it, not the frontmatter.
        let md = "# Voice transcription\n\n**Date:** 2026-05-22 16:08:26\n**Language:** Portuguese (BR)\n\n---\n\nHello world\n";
        assert_eq!(extract_preview(md), "Hello world");
    }

    #[test]
    fn extract_preview_uses_whole_content_when_no_separator() {
        let md = "Just a plain body, no separator.";
        assert_eq!(extract_preview(md), "Just a plain body, no separator.");
    }

    #[test]
    fn extract_preview_trims_surrounding_whitespace() {
        let md = "---\n\n   indented body   \n\n";
        assert_eq!(extract_preview(md), "indented body");
    }

    #[test]
    fn extract_preview_truncates_at_150_chars() {
        // 200 'a' chars after the separator -> output should be the
        // first 150 of them.
        let body = "a".repeat(200);
        let md = format!("---\n\n{}", body);
        let preview = extract_preview(&md);
        assert_eq!(preview.chars().count(), 150);
        assert!(preview.chars().all(|c| c == 'a'));
    }

    #[test]
    fn extract_preview_takes_first_block_after_first_separator() {
        // Multiple `---` in the file: preview comes from the segment
        // immediately after the first separator, not later sections.
        let md = "header\n---\nfirst body\n---\nsecond body";
        // Note: trim, then chars().take(150). The first body is just
        // "first body".
        assert_eq!(extract_preview(md), "first body");
    }

    #[test]
    fn extract_preview_handles_empty_content() {
        // Empty input -> empty preview. Tests the
        // `unwrap_or(content)` path when split returns nothing.
        assert_eq!(extract_preview(""), "");
    }

    #[test]
    fn extract_preview_counts_chars_not_bytes_for_multibyte_input() {
        // 60 PT-BR characters (mix of accented chars). Each accented
        // char is 2 UTF-8 bytes; if the truncation used .take(150) on
        // bytes, we'd cut mid-codepoint and panic. Using chars().take()
        // keeps it codepoint-safe.
        let body = "á".repeat(60);
        let md = format!("---\n\n{}", body);
        let preview = extract_preview(&md);
        assert_eq!(preview.chars().count(), 60);
    }
}
