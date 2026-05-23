<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import RecordingView from "$lib/recording/RecordingView.svelte";
  import TranscriptionList from "$lib/transcriptions/TranscriptionList.svelte";

  interface IncompleteSession {
    session_id: string;
    started_at: string;
    total_chunks: number;
    transcribed_chunks: number;
    session_dir: string;
  }

  let modelReady = $state(false);
  let downloadProgress = $state(0);
  let downloading = $state(false);
  let initError = $state("");
  let incompleteSessions: IncompleteSession[] = $state([]);
  let recovering = $state(false);

  onMount(async () => {
    const modelExists = await invoke<boolean>("check_model_status");

    if (!modelExists) {
      downloading = true;
      const unlisten = await listen<number>(
        "model-download-progress",
        (event) => {
          downloadProgress = event.payload;
        },
      );

      try {
        await invoke("download_model");
        unlisten();
        downloading = false;
      } catch (e) {
        unlisten();
        downloading = false;
        initError = `Erro ao baixar modelo: ${e}`;
        return;
      }
    }

    try {
      await invoke("initialize_whisper");
      modelReady = true;

      // Check for incomplete sessions from a previous crash
      incompleteSessions = await invoke<IncompleteSession[]>(
        "check_incomplete_sessions",
      );
    } catch (e) {
      initError = `Erro ao carregar modelo: ${e}`;
    }
  });

  async function recoverSession(session: IncompleteSession) {
    recovering = true;
    try {
      await invoke("recover_session", { sessionDir: session.session_dir });
      incompleteSessions = incompleteSessions.filter(
        (s) => s.session_id !== session.session_id,
      );
    } catch (e) {
      initError = `Erro ao recuperar: ${e}`;
    }
    recovering = false;
  }

  async function dismissSession(session: IncompleteSession) {
    await invoke("dismiss_session", { sessionDir: session.session_dir });
    incompleteSessions = incompleteSessions.filter(
      (s) => s.session_id !== session.session_id,
    );
  }
</script>

<main>
  {#if initError}
    <div class="error-screen">
      <p class="error-icon">!</p>
      <p>{initError}</p>
    </div>
  {:else if downloading}
    <div class="download-screen">
      <h2>Baixando modelo Whisper</h2>
      <p class="model-name">ggml-large-v3-turbo-q5_0 (~547MB)</p>
      <div class="progress-bar">
        <div class="progress-fill" style="width: {downloadProgress * 100}%"></div>
      </div>
      <p class="progress-text">{Math.round(downloadProgress * 100)}%</p>
    </div>
  {:else if !modelReady}
    <div class="loading-screen">
      <p>Carregando modelo...</p>
    </div>
  {:else}
    {#if incompleteSessions.length > 0}
      <div class="recovery-banner">
        <h3>Sessao interrompida encontrada</h3>
        {#each incompleteSessions as session}
          <div class="recovery-item">
            <p>
              Inicio: {session.started_at} - {session.total_chunks} trecho(s) de
              audio
            </p>
            <div class="recovery-actions">
              <button
                class="recover-btn"
                disabled={recovering}
                onclick={() => recoverSession(session)}
              >
                {recovering ? "Recuperando..." : "Recuperar transcricao"}
              </button>
              <button
                class="dismiss-btn"
                disabled={recovering}
                onclick={() => dismissSession(session)}
              >
                Descartar
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    <RecordingView />
    <TranscriptionList />
    <div class="footer">
      <button class="settings-btn" onclick={() => goto("/settings")}>
        Configuracoes
      </button>
    </div>
  {/if}
</main>

<style>
  main {
    padding: var(--space-4);
    min-height: 100vh;
    box-sizing: border-box;
  }

  .download-screen,
  .loading-screen,
  .error-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 300px;
    gap: 12px;
  }

  .download-screen h2 {
    font-size: 16px;
    font-weight: 600;
    margin: 0;
  }

  .model-name {
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

  .progress-text {
    font-size: 13px;
    color: var(--text-muted);
    margin: 0;
  }

  .error-screen {
    color: var(--danger);
  }

  .error-icon {
    font-size: 32px;
    font-weight: bold;
    margin: 0;
  }

  .footer {
    margin-top: var(--space-4);
    text-align: center;
  }

  .settings-btn {
    background: none;
    border: 1px solid var(--border-strong);
    color: var(--text-muted);
    padding: 6px 16px;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 13px;
  }

  .settings-btn:hover {
    border-color: var(--accent-border);
    color: var(--text);
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
</style>
