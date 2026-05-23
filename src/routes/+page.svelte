<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { createModelStatus } from "$lib/model-status.svelte";
  import HistoryList from "$lib/history/HistoryList.svelte";
  import HistoryDetail from "$lib/history/HistoryDetail.svelte";
  import type { TranscriptionEntry } from "$lib/history/types";

  interface IncompleteSession {
    session_id: string;
    started_at: string;
    total_chunks: number;
    transcribed_chunks: number;
    session_dir: string;
  }

  const model = createModelStatus();

  let entries = $state<TranscriptionEntry[]>([]);
  let selectedPath = $state<string | null>(null);
  let listError = $state<string | null>(null);

  let incompleteSessions: IncompleteSession[] = $state([]);
  let recovering = $state(false);
  let recoveryError = $state("");
  let unlisteners: (() => void)[] = [];

  let selected = $derived(
    selectedPath === null
      ? null
      : entries.find((e) => e.path === selectedPath) ?? null,
  );

  async function refreshEntries() {
    try {
      const next = await invoke<TranscriptionEntry[]>("get_transcriptions");
      entries = next;
      listError = null;
      if (selectedPath !== null && !next.some((e) => e.path === selectedPath)) {
        selectedPath = null;
      }
    } catch (e) {
      listError = String(e);
    }
  }

  async function refreshIncomplete() {
    try {
      incompleteSessions = await invoke<IncompleteSession[]>(
        "check_incomplete_sessions",
      );
    } catch (e) {
      console.error("check_incomplete_sessions failed", e);
    }
  }

  onMount(async () => {
    unlisteners.push(await model.subscribe());
    await Promise.all([refreshEntries(), refreshIncomplete()]);

    // A completed transcription writes a new `.md` file. Refresh
    // the list (and the incomplete-sessions banner) so the user
    // sees the new entry without leaving the window.
    unlisteners.push(
      await listen("transcription-complete", async () => {
        await Promise.all([refreshEntries(), refreshIncomplete()]);
      }),
    );
  });

  onDestroy(() => {
    unlisteners.forEach((fn) => fn());
  });

  async function recoverSession(session: IncompleteSession) {
    recovering = true;
    recoveryError = "";
    try {
      await invoke("recover_session", { sessionDir: session.session_dir });
      incompleteSessions = incompleteSessions.filter(
        (s) => s.session_id !== session.session_id,
      );
    } catch (e) {
      recoveryError = `Recovery failed: ${e}`;
    }
    recovering = false;
  }

  async function dismissSession(session: IncompleteSession) {
    try {
      await invoke("dismiss_session", { sessionDir: session.session_dir });
      incompleteSessions = incompleteSessions.filter(
        (s) => s.session_id !== session.session_id,
      );
    } catch (e) {
      recoveryError = `Dismiss failed: ${e}`;
    }
  }

  function onSelect(path: string) {
    selectedPath = path;
  }
</script>

<main class="main">
  {#if model.current.kind === "downloading"}
    <div class="splash">
      <h2>Downloading Whisper model</h2>
      <p class="splash-model">ggml-large-v3-turbo-q5_0 (~547 MB)</p>
      <div class="progress-bar">
        <div
          class="progress-fill"
          style="width: {model.current.progress * 100}%"
        ></div>
      </div>
      <p class="splash-text">{Math.round(model.current.progress * 100)}%</p>
    </div>
  {:else if model.current.kind === "error"}
    <div class="splash splash-error">
      <p class="error-icon">!</p>
      <p>{model.current.message}</p>
    </div>
  {:else if model.current.kind === "unchecked"}
    <div class="splash">
      <p>Loading model…</p>
    </div>
  {:else}
    {#if incompleteSessions.length > 0}
      <div class="recovery-banner">
        <h3>Unfinished session found</h3>
        {#each incompleteSessions as session}
          <div class="recovery-item">
            <p>
              Started {session.started_at} — {session.total_chunks}
              {session.total_chunks === 1 ? "audio chunk" : "audio chunks"} captured
            </p>
            <div class="recovery-actions">
              <button
                class="recover-btn"
                disabled={recovering}
                onclick={() => recoverSession(session)}
              >
                {recovering ? "Recovering…" : "Recover transcription"}
              </button>
              <button
                class="dismiss-btn"
                disabled={recovering}
                onclick={() => dismissSession(session)}
              >
                Dismiss
              </button>
            </div>
          </div>
        {/each}
        {#if recoveryError}
          <p class="recovery-error">{recoveryError}</p>
        {/if}
      </div>
    {/if}

    <div class="panes">
      <aside class="list-pane">
        {#if listError}
          <div class="list-error">
            <p>Failed to load transcriptions: {listError}</p>
            <button class="retry-btn" onclick={refreshEntries}>Retry</button>
          </div>
        {:else if entries.length === 0}
          <div class="list-empty">
            <div class="glyph" aria-hidden="true">🎙</div>
            <p class="empty-title">No transcriptions yet</p>
            <p class="empty-hint">
              Press <kbd>⌘</kbd><kbd>⇧</kbd><kbd>R</kbd> to start recording.
            </p>
          </div>
        {:else}
          <HistoryList {entries} {selectedPath} {onSelect} />
        {/if}
      </aside>
      <section class="detail-pane">
        <HistoryDetail entry={selected} />
      </section>
    </div>
  {/if}
</main>

<style>
  .main {
    display: grid;
    grid-template-rows: auto 1fr;
    height: 100vh;
    width: 100vw;
    box-sizing: border-box;
    overflow: hidden;
  }

  .splash {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 60vh;
    gap: 12px;
    grid-row: 1 / span 2;
  }

  .splash h2 {
    font-size: 16px;
    font-weight: 600;
    margin: 0;
  }

  .splash-model,
  .splash-text {
    font-size: 12px;
    color: var(--text-muted);
    margin: 0;
  }

  .progress-bar {
    width: 100%;
    max-width: 280px;
    height: 6px;
    background: var(--surface-raised);
    border-radius: 3px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
    transition: width 0.3s ease;
  }

  .splash-error {
    color: var(--danger);
  }

  .error-icon {
    font-size: 32px;
    font-weight: bold;
    margin: 0;
  }

  .recovery-banner {
    margin: 0.75rem 0.75rem 0;
    background: var(--warning-bg);
    border: 1px solid var(--warning-border);
    border-radius: var(--radius-lg);
    padding: 0.75rem 0.85rem;
  }

  .recovery-banner h3 {
    font-size: 0.85rem;
    color: var(--warning);
    margin: 0 0 0.5rem;
  }

  .recovery-item p {
    font-size: 0.78rem;
    color: var(--text-muted);
    margin: 0 0 0.5rem;
  }

  .recovery-actions {
    display: flex;
    gap: 0.5rem;
  }

  .recover-btn {
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    color: var(--accent);
    padding: 0.3rem 0.85rem;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 0.75rem;
  }

  .recover-btn:hover {
    background: var(--accent-bg-strong);
  }

  .recover-btn:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .dismiss-btn {
    background: transparent;
    border: 1px solid var(--border-strong);
    color: var(--text-muted);
    padding: 0.3rem 0.85rem;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 0.75rem;
  }

  .dismiss-btn:hover {
    background: var(--surface-raised);
    color: var(--text);
  }

  .recovery-error {
    margin: 0.5rem 0 0;
    color: var(--danger);
    font-size: 0.75rem;
  }

  .panes {
    display: grid;
    grid-template-columns: 40% 60%;
    min-height: 0;
    height: 100%;
  }

  .list-pane {
    border-right: 1px solid var(--border);
    overflow-y: auto;
    min-height: 0;
    background: var(--surface-sunken);
  }

  .detail-pane {
    min-height: 0;
    overflow: hidden;
  }

  .list-empty,
  .list-error {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 2rem 1.5rem;
    text-align: center;
    gap: 0.5rem;
  }

  .glyph {
    font-size: 2.5rem;
    opacity: 0.6;
  }

  .empty-title {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
  }

  .empty-hint {
    margin: 0;
    font-size: 0.78rem;
    color: var(--text-muted);
    max-width: 28ch;
    line-height: 1.5;
  }

  .empty-hint kbd {
    display: inline-block;
    padding: 0.05em 0.4em;
    margin: 0 1px;
    font-family: inherit;
    font-size: 0.85em;
    background: var(--surface-raised);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
  }

  .list-error p {
    margin: 0;
    color: var(--danger);
    font-size: 0.8rem;
  }

  .retry-btn {
    appearance: none;
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    color: var(--accent);
    padding: 0.3rem 0.85rem;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 0.78rem;
  }

  .retry-btn:hover {
    background: var(--accent-bg-strong);
  }
</style>
