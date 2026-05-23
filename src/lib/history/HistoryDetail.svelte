<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import type { TranscriptionEntry } from "./types";
  import {
    displayDate,
    formatDuration,
    parseTrackedBody,
    trackChip,
  } from "./format";

  interface Props {
    entry: TranscriptionEntry | null;
  }

  let { entry }: Props = $props();

  let body = $state<string | null>(null);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let copyState = $state<"idle" | "copied" | "error">("idle");
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  let sections = $derived(body === null ? [] : parseTrackedBody(body));

  $effect(() => {
    const current = entry;
    if (current === null) {
      body = null;
      loadError = null;
      return;
    }
    loading = true;
    loadError = null;
    body = null;
    invoke<string>("get_transcription_body", { path: current.path })
      .then((b) => {
        if (entry?.path === current.path) {
          body = b;
        }
      })
      .catch((e) => {
        if (entry?.path === current.path) {
          loadError = String(e);
        }
      })
      .finally(() => {
        if (entry?.path === current.path) {
          loading = false;
        }
      });
  });

  async function copyTranscript() {
    if (!body) return;
    try {
      await navigator.clipboard.writeText(body);
      copyState = "copied";
    } catch {
      copyState = "error";
    }
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => {
      copyState = "idle";
    }, 1500);
  }

  async function reveal() {
    if (!entry) return;
    try {
      await revealItemInDir(entry.path);
    } catch (e) {
      console.error("revealItemInDir failed", e);
    }
  }
</script>

{#if entry === null}
  <div class="empty">
    <p>Select a transcription to read.</p>
  </div>
{:else}
  {@const dateInfo = displayDate(entry.date, entry.filename)}
  {@const dur = formatDuration(entry.duration_seconds)}
  <article class="detail">
    <header class="head">
      <div class="head-text">
        <h1 class="title">{dateInfo.full}</h1>
        {#if dur}
          <p class="subtitle">Duration {dur}</p>
        {/if}
      </div>
      <div class="actions">
        <button
          type="button"
          class="btn"
          onclick={copyTranscript}
          disabled={body === null || loading}
        >
          {#if copyState === "copied"}
            Copied
          {:else if copyState === "error"}
            Copy failed
          {:else}
            Copy
          {/if}
        </button>
        <button type="button" class="btn" onclick={reveal}>
          Reveal in Finder
        </button>
        <button type="button" class="btn delete" disabled title="Coming in ui-04">
          Delete
        </button>
      </div>
    </header>

    {#if entry.date || entry.language || entry.mic_device || entry.sys_device || entry.chunks !== null}
      <dl class="meta">
        {#if entry.date}
          <dt>Date</dt>
          <dd>{entry.date}</dd>
        {/if}
        {#if dur}
          <dt>Duration</dt>
          <dd>{dur}</dd>
        {/if}
        {#if entry.language}
          <dt>Language</dt>
          <dd>{entry.language}</dd>
        {/if}
        {#if entry.mic_device}
          <dt>Microphone</dt>
          <dd>{entry.mic_device}</dd>
        {/if}
        {#if entry.sys_device}
          <dt>System audio</dt>
          <dd>{entry.sys_device}</dd>
        {/if}
        {#if entry.chunks !== null && entry.chunks !== undefined}
          <dt>Chunks</dt>
          <dd>{entry.chunks}</dd>
        {/if}
      </dl>
    {/if}

    <section class="body">
      {#if loading}
        <p class="placeholder">Loading…</p>
      {:else if loadError}
        <p class="error">Failed to load transcript: {loadError}</p>
      {:else if body !== null}
        {#each sections as section}
          {#if section.label !== null && sections.length > 1}
            {@const chip = trackChip(section.label) ?? section.label}
            <div class="track">
              <span class="track-chip">{chip}</span>
              <p class="track-text">{section.text}</p>
            </div>
          {:else}
            <p class="track-text plain">{section.text}</p>
          {/if}
        {/each}
      {/if}
    </section>
  </article>
{/if}

<style>
  .empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .detail {
    height: 100%;
    overflow-y: auto;
    padding: 1.25rem 1.5rem 2rem;
    box-sizing: border-box;
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 1.1rem;
  }

  .title {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .subtitle {
    margin: 0.2rem 0 0;
    font-size: 0.78rem;
    color: var(--text-muted);
  }

  .actions {
    display: flex;
    gap: 0.4rem;
    flex-shrink: 0;
  }

  .btn {
    appearance: none;
    background: var(--surface-raised);
    border: 1px solid var(--border-strong);
    color: var(--text);
    padding: 0.3rem 0.7rem;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 0.78rem;
    transition: background 80ms ease, border-color 80ms ease;
  }

  .btn:hover:not(:disabled) {
    background: var(--accent-bg);
    border-color: var(--accent-border);
    color: var(--accent);
  }

  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .btn.delete {
    color: var(--danger);
    border-color: var(--danger-border);
  }

  .btn.delete:hover:not(:disabled) {
    background: var(--danger-bg);
    color: var(--danger);
  }

  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    column-gap: 1rem;
    row-gap: 0.45rem;
    margin: 0 0 1.25rem;
    padding: 0.85rem 1rem;
    background: var(--surface-sunken);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    font-size: 0.82rem;
  }

  .meta dt {
    text-transform: uppercase;
    font-size: 0.66rem;
    letter-spacing: 0.06em;
    font-weight: 600;
    color: var(--text-muted);
    align-self: center;
  }

  .meta dd {
    margin: 0;
    word-break: break-word;
    color: var(--text);
  }

  .body {
    border-top: 1px solid var(--border);
    padding-top: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .placeholder {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.85rem;
  }

  .error {
    color: var(--danger);
    margin: 0;
  }

  .track {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .track-chip {
    align-self: flex-start;
    font-size: 0.62rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    padding: 0.1rem 0.45rem;
    border-radius: var(--radius-sm);
    background: var(--accent-bg);
    color: var(--accent);
  }

  .track-text {
    margin: 0;
    font-size: 0.92rem;
    line-height: 1.6;
    white-space: pre-wrap;
    color: var(--text);
  }

  .track-text.plain {
    margin: 0;
  }
</style>
