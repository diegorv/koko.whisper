<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { createModelStatus } from "$lib/model-status.svelte";

  interface ChunkEvent {
    track: string;
    transcript: string;
  }

  let isRecording = $state(false);
  let isProcessing = $state(false);
  let statusText = $state("");
  let micTranscript = $state("");
  let sysTranscript = $state("");
  let chunkCount = $state(0);
  let elapsedSeconds = $state(0);
  let timerInterval: ReturnType<typeof setInterval> | null = null;

  const model = createModelStatus();
  let modelReady = $derived(model.current.kind === "ready");

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
    unlisteners.push(await model.subscribe());

    try {
      const [status, elapsed] = await invoke<[number, number]>("get_app_status");
      if (status === 1) {
        isRecording = true;
        elapsedSeconds = elapsed;
        startTimer();
      } else if (status === 2) {
        isProcessing = true;
        statusText = "Transcribing…";
      }
    } catch {}

    unlisteners.push(
      await listen("recording-started", () => {
        isRecording = true;
        isProcessing = false;
        micTranscript = "";
        sysTranscript = "";
        chunkCount = 0;
        startTimer();
      }),
    );

    unlisteners.push(
      await listen<string>("transcription-complete", () => {
        isRecording = false;
        isProcessing = false;
        stopTimer();
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
          statusText = "Processing audio…";
        } else if (p === "transcribing") {
          statusText = "Transcribing…";
        } else if (p === "recovering") {
          statusText = "Recovering session…";
        } else if (p.startsWith("transcribing")) {
          statusText = "Transcribing chunk…";
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
    if (!modelReady) return;

    if (isRecording) {
      isRecording = false;
      stopTimer();
      isProcessing = true;
      statusText = "Stopping…";
      try {
        await invoke<string>("stop_recording");
      } catch (e) {
        statusText = `Error: ${e}`;
        isProcessing = false;
      }
    } else {
      try {
        await invoke("start_recording");
      } catch (e) {
        statusText = `Error: ${e}`;
      }
    }
  }
</script>

<div class="recording-view">
  {#if model.current.kind === "downloading"}
    <div class="splash">
      <h3>Downloading Whisper model</h3>
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
    <button
      class="record-btn"
      class:recording={isRecording}
      class:processing={isProcessing}
      disabled={isProcessing}
      aria-label={isRecording ? "Stop recording" : "Start recording"}
      onclick={toggleRecording}
    >
      <span class="record-icon" class:pulse={isRecording}></span>
    </button>

    {#if isRecording}
      <p class="timer">{formatTime(elapsedSeconds)}</p>
    {/if}

    <p class="status">
      {#if isRecording}
        Recording <span class="hint">(⌘⇧R to stop)</span>
      {:else if isProcessing}
        {statusText}
      {:else}
        Click or press ⌘⇧R to record
      {/if}
    </p>

    {#if isRecording && hasPartialTranscript}
      <div class="partial-transcript">
        <h3>
          Live transcript ({chunkCount}
          {chunkCount === 1 ? "chunk" : "chunks"})
        </h3>
        {#if hasBothTracks}
          {#if micTranscript}
            <div class="track-section">
              <span class="track-label">You</span>
              <p>{micTranscript}</p>
            </div>
          {/if}
          {#if sysTranscript}
            <div class="track-section">
              <span class="track-label">Other</span>
              <p>{sysTranscript}</p>
            </div>
          {/if}
        {:else}
          <p>{micTranscript || sysTranscript}</p>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .recording-view {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: var(--space-5) var(--space-4);
    min-height: 100vh;
    box-sizing: border-box;
  }

  .record-btn {
    width: 72px;
    height: 72px;
    border-radius: 50%;
    border: 3px solid var(--border-strong);
    background: var(--surface-raised);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .record-btn:hover {
    border-color: var(--accent-border);
  }

  .record-btn.recording {
    border-color: #ff4444;
  }

  .record-btn.processing {
    border-color: var(--accent);
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
    font-family: var(--font-mono);
  }

  .status {
    font-size: 13px;
    color: var(--text-muted);
    margin-top: 8px;
    text-align: center;
  }

  .hint {
    font-size: 11px;
    color: var(--text-faint);
  }

  .partial-transcript {
    margin-top: 20px;
    width: 100%;
    padding: 12px;
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    border-radius: var(--radius-lg);
    box-sizing: border-box;
    max-height: 180px;
    overflow-y: auto;
  }

  .partial-transcript h3 {
    font-size: 12px;
    color: var(--accent);
    margin: 0 0 8px 0;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .partial-transcript p {
    font-size: 14px;
    line-height: 1.5;
    margin: 0;
    color: var(--text);
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
    border-radius: var(--radius-sm);
    margin-bottom: 4px;
    background: var(--surface-raised);
    color: var(--text-muted);
  }

  .splash {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    flex: 1;
    gap: 12px;
    width: 100%;
  }

  .splash h3 {
    font-size: 14px;
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

  .splash-error {
    color: var(--danger);
    text-align: center;
  }

  .error-icon {
    font-size: 32px;
    font-weight: bold;
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
</style>
