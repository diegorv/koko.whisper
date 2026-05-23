<script lang="ts">
  import type { TranscriptionEntry } from "./types";
  import { displayDate, formatDuration } from "./format";

  interface Props {
    entries: TranscriptionEntry[];
    selectedPath: string | null;
    flashingPath?: string | null;
    onSelect: (path: string) => void;
  }

  let { entries, selectedPath, flashingPath = null, onSelect }: Props = $props();
</script>

<ul class="list" aria-label="Transcriptions">
  {#each entries as entry (entry.path)}
    {@const { short } = displayDate(entry.date, entry.filename)}
    {@const dur = formatDuration(entry.duration_seconds)}
    <li>
      <button
        type="button"
        class="row"
        class:selected={selectedPath === entry.path}
        class:flash={flashingPath === entry.path}
        aria-pressed={selectedPath === entry.path}
        onclick={() => onSelect(entry.path)}
      >
        <div class="row-head">
          <span class="row-date">{short}</span>
          {#if dur}
            <span class="row-duration">{dur}</span>
          {/if}
        </div>
        {#if entry.preview}
          <p class="row-preview">{entry.preview}</p>
        {/if}
      </button>
    </li>
  {/each}
</ul>

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0.25rem 0.25rem 1rem;
  }

  .row {
    display: block;
    width: 100%;
    padding: 0.55rem 0.75rem;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius);
    text-align: left;
    cursor: pointer;
    color: inherit;
    font: inherit;
    transition:
      background 80ms ease,
      border-color 80ms ease;
  }

  .row:hover {
    background: var(--surface-raised);
  }

  .row.selected {
    background: var(--accent-bg);
    border-color: var(--accent-border);
  }

  .row.flash {
    animation: flash-pulse 200ms ease-out;
  }

  @keyframes flash-pulse {
    0% {
      background: var(--accent-bg-strong);
    }
    100% {
      background: var(--accent-bg);
    }
  }

  .row-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .row-date {
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }

  .row-duration {
    font-size: 0.72rem;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    font-family: var(--font-mono);
  }

  .row-preview {
    margin: 0.25rem 0 0;
    font-size: 0.78rem;
    color: var(--text-muted);
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
</style>
