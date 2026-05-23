<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import TranscriptionList from "$lib/transcriptions/TranscriptionList.svelte";
  import { createModelStatus } from "$lib/model-status.svelte";

  interface IncompleteSession {
    session_id: string;
    started_at: string;
    total_chunks: number;
    transcribed_chunks: number;
    session_dir: string;
  }

  const model = createModelStatus();
  let incompleteSessions: IncompleteSession[] = $state([]);
  let recovering = $state(false);
  let recoveryError = $state("");
  let unlisteners: (() => void)[] = [];

  onMount(async () => {
    unlisteners.push(await model.subscribe());

    try {
      incompleteSessions = await invoke<IncompleteSession[]>(
        "check_incomplete_sessions",
      );
    } catch (e) {
      console.error("check_incomplete_sessions failed", e);
    }

    // After a successful recovery a new transcription lands on disk;
    // refresh the incomplete list so the recovered row disappears.
    unlisteners.push(
      await listen("transcription-complete", async () => {
        try {
          incompleteSessions = await invoke<IncompleteSession[]>(
            "check_incomplete_sessions",
          );
        } catch {}
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
</script>

<main>
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

    <div class="placeholder">
      <h1>History</h1>
      <p>The two-pane browser lands in ui-03.</p>
      <p class="hint">
        Press <kbd>⌘</kbd><kbd>⇧</kbd><kbd>R</kbd> to start recording, or open Settings via the tray.
      </p>
    </div>

    <TranscriptionList />
  {/if}
</main>

<style>
  main {
    padding: var(--space-4);
    min-height: 100vh;
    box-sizing: border-box;
  }

  .placeholder {
    margin: var(--space-5) auto var(--space-4);
    text-align: center;
    max-width: 480px;
  }

  .placeholder h1 {
    margin: 0 0 var(--space-2);
    font-size: 1.4rem;
    font-weight: 600;
  }

  .placeholder p {
    margin: 0 0 var(--space-2);
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .placeholder .hint {
    color: var(--text-faint);
    font-size: 0.8rem;
  }

  .placeholder kbd {
    display: inline-block;
    padding: 0.05em 0.4em;
    margin: 0 1px;
    font-family: inherit;
    font-size: 0.85em;
    background: var(--surface-raised);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
  }

  .splash {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 60vh;
    gap: 12px;
  }

  .splash h2 {
    font-size: 16px;
    font-weight: 600;
    margin: 0;
  }

  .splash-model {
    font-size: 12px;
    color: var(--text-muted);
    margin: 0;
  }

  .splash-text {
    font-size: 13px;
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
    background: var(--warning-bg);
    border: 1px solid var(--warning-border);
    border-radius: var(--radius-lg);
    padding: 12px;
    margin-bottom: 16px;
  }

  .recovery-banner h3 {
    font-size: 13px;
    color: var(--warning);
    margin: 0 0 8px 0;
  }

  .recovery-item p {
    font-size: 12px;
    color: var(--text-muted);
    margin: 0 0 8px 0;
  }

  .recovery-actions {
    display: flex;
    gap: 8px;
  }

  .recover-btn {
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    color: var(--accent);
    padding: 6px 14px;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 12px;
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
    padding: 6px 14px;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 12px;
  }

  .dismiss-btn:hover {
    background: var(--surface-raised);
    color: var(--text);
  }

  .recovery-error {
    margin: 8px 0 0;
    color: var(--danger);
    font-size: 12px;
  }
</style>
