// Shape of the row served by the Rust `get_transcriptions` command.
// Every meta field is optional — the Detail pane hides rows whose
// underlying field is `null` so legacy `.md` files without the
// expanded frontmatter degrade gracefully (ADR-0002 §9).
export interface TranscriptionEntry {
  filename: string;
  path: string;
  preview: string;
  date: string | null;
  duration_seconds: number | null;
  language: string | null;
  mic_device: string | null;
  sys_device: string | null;
  chunks: number | null;
}
