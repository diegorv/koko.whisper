// Pure formatters shared between the list row and the detail pane.

export function formatDuration(seconds: number | null): string {
  if (seconds === null || seconds < 0) return "";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  if (h > 0) return `${h}:${mm}:${ss}`;
  return `${mm}:${ss}`;
}

/// Pull the date from frontmatter when present, otherwise parse the
/// filename which is `YYYY-MM-DD_HH-MM-SS.md`. Returns the original
/// string when no parse succeeds (so we never show a blank cell).
export function displayDate(
  date: string | null,
  filename: string,
): { full: string; short: string } {
  if (date) {
    return { full: date, short: shortDate(date) };
  }
  const stripped = filename.replace(/\.md$/, "");
  const match = stripped.match(
    /^(\d{4})-(\d{2})-(\d{2})_(\d{2})-(\d{2})-(\d{2})$/,
  );
  if (match) {
    const [, y, mo, d, h, m, s] = match;
    const full = `${y}-${mo}-${d} ${h}:${m}:${s}`;
    return { full, short: `${y}-${mo}-${d} ${h}:${m}` };
  }
  return { full: stripped, short: stripped };
}

function shortDate(s: string): string {
  // Trim "YYYY-MM-DD HH:MM:SS" → "YYYY-MM-DD HH:MM"
  if (/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}/.test(s)) {
    return s.slice(0, 16);
  }
  return s;
}

// Parse "YYYY-MM-DD HH:MM:SS" (frontmatter format) or the filename
// stem "YYYY-MM-DD_HH-MM-SS" into epoch ms. Returns null when the
// input is not in either shape.
export function parseLocalTimestamp(s: string | null): number | null {
  if (!s) return null;
  const cleaned = s.replace("_", " ").replace(/-(\d{2})-(\d{2})$/, ":$1:$2");
  const iso = cleaned.replace(" ", "T");
  const t = Date.parse(iso);
  return Number.isNaN(t) ? null : t;
}

/// Render an epoch timestamp as a short relative phrase: "just now"
/// under a minute, "Nm ago" under an hour, "Nh ago" under a day, "Nd
/// ago" otherwise. Returns null when the input could not be parsed
/// so callers can hide the row instead of rendering a blank.
export function relativeTime(ms: number, now: number): string {
  const diff = Math.max(0, now - ms);
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return "just now";
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  return `${day}d ago`;
}

// Split a multi-track body (`## Microphone` / `## System audio`)
// into labelled chunks. Single-track bodies return a single chunk
// with `label: null`.
export function parseTrackedBody(
  body: string,
): Array<{ label: string | null; text: string }> {
  // Look for headings that start with `## ` at line start.
  const sections: Array<{ label: string | null; text: string }> = [];
  const lines = body.split("\n");
  let currentLabel: string | null = null;
  let buffer: string[] = [];

  const flush = () => {
    const text = buffer.join("\n").trim();
    if (text.length > 0 || sections.length > 0) {
      sections.push({ label: currentLabel, text });
    }
  };

  for (const line of lines) {
    const match = line.match(/^##\s+(.+)$/);
    if (match) {
      flush();
      currentLabel = match[1].trim();
      buffer = [];
    } else {
      buffer.push(line);
    }
  }
  flush();

  // If we never saw a heading, return the whole body as a single
  // unlabelled section.
  if (sections.length === 0) {
    return [{ label: null, text: body.trim() }];
  }
  if (sections.length === 1 && sections[0].label === null) {
    return [{ label: null, text: sections[0].text }];
  }
  return sections;
}

// Map the Rust `display_label()` track labels to the friendlier
// "You" / "Other" chips rendered in the Detail body.
export function trackChip(label: string | null): string | null {
  if (label === null) return null;
  const lower = label.toLowerCase();
  if (lower.includes("microphone") || lower.includes("microfone")) return "You";
  if (lower.includes("system")) return "Other";
  return label;
}
