<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { onMount, onDestroy } from "svelte";

  interface TranscriptionEntry {
    filename: string;
    preview: string;
    path: string;
  }

  let transcriptions: TranscriptionEntry[] = $state([]);
  let unlisten: (() => void) | null = null;

  onMount(async () => {
    await refresh();
    unlisten = await listen("transcription-complete", async () => {
      await refresh();
    });
  });

  onDestroy(() => {
    unlisten?.();
  });

  async function refresh() {
    transcriptions = await invoke<TranscriptionEntry[]>("get_transcriptions");
  }
</script>

{#if transcriptions.length > 0}
  <div class="transcription-list">
    <h3>Transcricoes recentes</h3>
    {#each transcriptions as entry}
      <button class="entry" onclick={() => revealItemInDir(entry.path)}>
        <span class="filename">{entry.filename}</span>
        <p class="preview">{entry.preview}{entry.preview.length >= 150 ? "..." : ""}</p>
      </button>
    {/each}
  </div>
{/if}

<style>
  .transcription-list {
    margin-top: 16px;
  }

  .transcription-list h3 {
    font-size: 12px;
    color: var(--text-muted);
    margin: 0 0 8px 0;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .entry {
    display: block;
    width: 100%;
    padding: 10px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    margin-bottom: 6px;
    cursor: pointer;
    text-align: left;
    font-family: inherit;
    color: inherit;
  }

  .entry:hover {
    border-color: var(--accent-border);
    background: var(--accent-bg);
  }

  .filename {
    font-size: 11px;
    color: var(--text-faint);
    font-family: var(--font-mono);
  }

  .preview {
    font-size: 13px;
    color: var(--text-muted);
    margin: 4px 0 0 0;
    line-height: 1.4;
  }
</style>
