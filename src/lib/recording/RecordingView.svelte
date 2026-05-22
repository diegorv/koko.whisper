<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";

  interface ChunkEvent {
    track: string;
    transcript: string;
  }

  let isRecording = $state(false);
  let isProcessing = $state(false);
  let statusText = $state("");
  let lastTranscription = $state("");
  let micTranscript = $state("");
  let sysTranscript = $state("");
  let chunkCount = $state(0);
  let elapsedSeconds = $state(0);
  let timerInterval: ReturnType<typeof setInterval> | null = null;

  let hasPartialTranscript = $derived(
    micTranscript.length > 0 || sysTranscript.length > 0,
  );
  let hasBothTracks = $derived(
    micTranscript.length > 0 && sysTranscript.length > 0,
  );

  let unlisteners: (() => void)[] = [];

  function formatTime(totalSeconds: number): string {
    const h = Math.floor(totalSeconds / 3600);
    const m = Math.floor((totalSeconds % 3600) / 60);
    const s = totalSeconds % 60;
    const mm = String(m).padStart(2, "0");
    const ss = String(s).padStart(2, "0");
    if (h > 0) {
      return `${h}:${mm}:${ss}`;
    }
    return `${mm}:${ss}`;
  }

  function startTimer() {
    elapsedSeconds = 0;
    timerInterval = setInterval(() => {
      elapsedSeconds++;
    }, 1000);
  }

  function stopTimer() {
    if (timerInterval) {
      clearInterval(timerInterval);
      timerInterval = null;
    }
  }

  onMount(async () => {
    // Sync state from backend (recording may have started from tray/shortcut)
    try {
      const [status, elapsed] = await invoke<[number, number]>("get_app_status");
      if (status === 1) {
        // STATUS_RECORDING
        isRecording = true;
        elapsedSeconds = elapsed;
        startTimer();
      } else if (status === 2) {
        // STATUS_TRANSCRIBING
        isProcessing = true;
        statusText = "Transcrevendo...";
      }
    } catch {}

    // Recording started (from tray, shortcut, or this window)
    unlisteners.push(
      await listen("recording-started", () => {
        isRecording = true;
        isProcessing = false;
        lastTranscription = "";
        micTranscript = "";
        sysTranscript = "";
        chunkCount = 0;
        startTimer();
      }),
    );

    unlisteners.push(
      await listen<string>("transcription-complete", (event) => {
        isRecording = false;
        isProcessing = false;
        stopTimer();
        lastTranscription = event.payload;
        statusText = "";
        micTranscript = "";
        sysTranscript = "";
        chunkCount = 0;
      }),
    );

    unlisteners.push(
      await listen<string>("transcription-status", (event) => {
        const p = event.payload;
        if (p === "resampling" || p === "processing") {
          isRecording = false;
          isProcessing = true;
          stopTimer();
          statusText = "Processando audio...";
        } else if (p === "transcribing") {
          statusText = "Transcrevendo...";
        } else if (p === "recovering") {
          statusText = "Recuperando sessao...";
        } else if (p.startsWith("transcribing")) {
          statusText = "Transcrevendo trecho...";
        }
      }),
    );

    unlisteners.push(
      await listen<ChunkEvent>("chunk-transcribed", (event) => {
        const { track, transcript } = event.payload;
        chunkCount++;
        if (track === "microphone") {
          micTranscript = micTranscript
            ? micTranscript + " " + transcript
            : transcript;
        } else if (track === "system") {
          sysTranscript = sysTranscript
            ? sysTranscript + " " + transcript
            : transcript;
        }
      }),
    );
  });

  onDestroy(() => {
    stopTimer();
    unlisteners.forEach((fn) => fn());
  });

  async function toggleRecording() {
    if (isProcessing) return;

    if (isRecording) {
      // Immediate UI feedback while backend processes
      isRecording = false;
      stopTimer();
      isProcessing = true;
      statusText = "Parando...";
      try {
        await invoke<string>("stop_recording");
      } catch (e) {
        statusText = `Erro: ${e}`;
        isProcessing = false;
      }
      // Final state set by transcription-complete event
    } else {
      try {
        await invoke("start_recording");
      } catch (e) {
        statusText = `Erro: ${e}`;
      }
      // UI state set by recording-started event
    }
  }
</script>

<div class="recording-view">
  <button
    class="record-btn"
    class:recording={isRecording}
    class:processing={isProcessing}
    disabled={isProcessing}
    onclick={toggleRecording}
  >
    <span class="record-icon" class:pulse={isRecording}></span>
  </button>

  {#if isRecording}
    <p class="timer">{formatTime(elapsedSeconds)}</p>
  {/if}

  <p class="status">
    {#if isRecording}
      Gravando <span class="hint">(Cmd+Shift+R para parar)</span>
    {:else if isProcessing}
      {statusText}
    {:else}
      Clique ou Cmd+Shift+R para gravar
    {/if}
  </p>

  {#if isRecording && hasPartialTranscript}
    <div class="partial-transcript">
      <h3>
        Transcricao parcial ({chunkCount}
        {chunkCount === 1 ? "trecho" : "trechos"})
      </h3>
      {#if hasBothTracks}
        {#if micTranscript}
          <div class="track-section">
            <span class="track-label mic">Eu</span>
            <p>{micTranscript}</p>
          </div>
        {/if}
        {#if sysTranscript}
          <div class="track-section">
            <span class="track-label sys">Participante</span>
            <p>{sysTranscript}</p>
          </div>
        {/if}
      {:else}
        <p>{micTranscript || sysTranscript}</p>
      {/if}
    </div>
  {/if}

  {#if lastTranscription}
    <div class="last-transcription">
      <h3>Ultima transcricao</h3>
      <p>{lastTranscription}</p>
    </div>
  {/if}
</div>

<style>
  .recording-view {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 24px 0;
  }

  .record-btn {
    width: 72px;
    height: 72px;
    border-radius: 50%;
    border: 3px solid #444;
    background: #2a2a2a;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .record-btn:hover {
    border-color: #666;
  }

  .record-btn.recording {
    border-color: #ff4444;
  }

  .record-btn.processing {
    border-color: #4a9eff;
    opacity: 0.7;
    cursor: wait;
  }

  .record-icon {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: #ff4444;
    transition: all 0.2s;
  }

  .record-btn.recording .record-icon {
    border-radius: 4px;
    width: 24px;
    height: 24px;
  }

  .pulse {
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.5;
    }
  }

  .timer {
    font-size: 28px;
    font-weight: 600;
    color: #ff4444;
    margin: 12px 0 0 0;
    font-variant-numeric: tabular-nums;
    letter-spacing: 1px;
    font-family: "SF Mono", "Menlo", "Monaco", monospace;
  }

  .status {
    font-size: 13px;
    color: #888;
    margin-top: 8px;
    text-align: center;
  }

  .hint {
    font-size: 11px;
    color: #666;
  }

  .partial-transcript {
    margin-top: 20px;
    width: 100%;
    padding: 12px;
    background: #1e2a1e;
    border: 1px solid #2a3a2a;
    border-radius: 8px;
    box-sizing: border-box;
    max-height: 300px;
    overflow-y: auto;
  }

  .partial-transcript h3 {
    font-size: 12px;
    color: #6a6;
    margin: 0 0 8px 0;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .partial-transcript p {
    font-size: 14px;
    line-height: 1.5;
    margin: 0;
    color: #ccc;
  }

  .track-section {
    margin-bottom: 12px;
  }

  .track-section:last-child {
    margin-bottom: 0;
  }

  .track-label {
    display: inline-block;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 2px 6px;
    border-radius: 3px;
    margin-bottom: 4px;
  }

  .track-label.mic {
    background: #2a3a2a;
    color: #8c8;
  }

  .track-label.sys {
    background: #2a2a3a;
    color: #88c;
  }

  .last-transcription {
    margin-top: 20px;
    width: 100%;
    padding: 12px;
    background: #222;
    border-radius: 8px;
    box-sizing: border-box;
  }

  .last-transcription h3 {
    font-size: 12px;
    color: #666;
    margin: 0 0 8px 0;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .last-transcription p {
    font-size: 14px;
    line-height: 1.5;
    margin: 0;
    color: #ccc;
  }
</style>
