<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, onDestroy } from "svelte";
  import { createModelStatus } from "$lib/model-status.svelte";

  interface ChunkEvent {
    track: string;
    transcript: string;
  }

  let isRecording = $state(false);
  let isProcessing = $state(false);
  let statusText = $state("");
  let startError = $state<string | null>(null);
  let chunkCount = $state(0);
  let elapsedSeconds = $state(0);
  let timerInterval: ReturnType<typeof setInterval> | null = null;

  const model = createModelStatus();
  let modelReady = $derived(model.current.kind === "ready");

  let unlisteners: (() => void)[] = [];

  function formatTime(totalSeconds: number): string {
    const h = Math.floor(totalSeconds / 3600);
    const m = Math.floor((totalSeconds % 3600) / 60);
    const s = totalSeconds % 60;
    const mm = String(m).padStart(2, "0");
    const ss = String(s).padStart(2, "0");
    if (h > 0) return `${h}:${mm}:${ss}`;
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
        startError = null;
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
          statusText = "Processing…";
        } else if (p === "transcribing") {
          statusText = "Transcribing…";
        } else if (p === "recovering") {
          statusText = "Recovering…";
        }
      }),
    );

    unlisteners.push(
      await listen<ChunkEvent>("chunk-transcribed", () => {
        chunkCount++;
      }),
    );
  });

  onDestroy(() => {
    stopTimer();
    unlisteners.forEach((fn) => fn());
  });

  async function startRecording() {
    startError = null;
    try {
      await invoke("start_recording");
    } catch (e) {
      startError = String(e);
    }
  }

  // Manual drag handler. The `data-tauri-drag-region` attribute is
  // unreliable on macOS frameless+transparent windows; calling
  // `startDragging()` on pointerdown is the workaround that works
  // both ways. Skipped when the event target is interactive so the
  // record button keeps receiving its click.
  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement | null;
    if (!target) return;
    if (target.closest("button, input, textarea, a, [role='button']")) {
      return;
    }
    event.preventDefault();
    void getCurrentWindow().startDragging();
  }

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
      await startRecording();
    }
  }

  let primaryLabel = $derived.by(() => {
    if (model.current.kind === "downloading") return "Loading model";
    if (model.current.kind === "error") return "Model error";
    if (model.current.kind === "unchecked") return "Loading…";
    if (isProcessing) return statusText || "Working…";
    if (isRecording) return formatTime(elapsedSeconds);
    return "Ready";
  });

  let secondaryLabel = $derived.by(() => {
    if (model.current.kind === "downloading")
      return `${Math.round(model.current.progress * 100)}%`;
    if (model.current.kind === "error") return model.current.message;
    if (isRecording) {
      if (chunkCount > 0) {
        return `${chunkCount} ${chunkCount === 1 ? "chunk" : "chunks"} · ⌘⇧R to stop`;
      }
      return "⌘⇧R to stop";
    }
    if (isProcessing) return "";
    return "⌘⇧R to record";
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions — the drag
     handler is opt-in pointer behaviour; the visible interactive
     control inside (.record-btn) keeps its own button semantics. -->
<div class="pill" onpointerdown={onPointerDown}>
  {#if startError}
    <div class="error-strip" role="alert">
      <p class="error-msg">{startError}</p>
      <button class="retry-btn" type="button" onclick={startRecording}>
        Retry
      </button>
    </div>
  {/if}

  <div class="pill-body" data-tauri-drag-region>
    <button
      class="record-btn"
      class:recording={isRecording}
      class:processing={isProcessing}
      class:disabled={!modelReady}
      disabled={isProcessing || !modelReady}
      aria-label={isRecording ? "Stop recording" : "Start recording"}
      onclick={toggleRecording}
    >
      <span class="record-icon" class:pulse={isRecording}></span>
    </button>

    <div class="labels">
      <span class="primary" class:recording={isRecording}>{primaryLabel}</span>
      {#if secondaryLabel}
        <span class="sep" aria-hidden="true">·</span>
        <span class="secondary">{secondaryLabel}</span>
      {/if}
    </div>

    {#if model.current.kind === "downloading"}
      <div
        class="progress-ring"
        aria-label="Download progress"
        style="--progress: {model.current.progress * 100}%"
      ></div>
    {/if}
  </div>
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
  }

  .pill {
    width: 100vw;
    height: 100vh;
    box-sizing: border-box;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow: hidden;
  }

  .pill-body {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0 0.7rem 0 0.4rem;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    box-shadow:
      0 1px 2px rgba(0, 0, 0, 0.08),
      0 8px 24px rgba(0, 0, 0, 0.18);
    flex: 1;
    min-height: 0;
  }

  .record-btn {
    appearance: none;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    border: 2px solid var(--border-strong);
    background: var(--surface-raised);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    transition: border-color 120ms ease, transform 120ms ease;
    padding: 0;
  }

  .record-btn:hover:not(:disabled) {
    border-color: var(--accent-border);
    transform: scale(1.04);
  }

  .record-btn.recording {
    border-color: #ff4444;
  }

  .record-btn.processing {
    border-color: var(--accent);
    opacity: 0.7;
    cursor: wait;
  }

  .record-btn.disabled {
    opacity: 0.45;
    cursor: default;
  }

  .record-icon {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #ff4444;
    transition: border-radius 120ms ease, width 120ms ease, height 120ms ease;
  }

  .record-btn.recording .record-icon {
    border-radius: 2px;
    width: 10px;
    height: 10px;
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
      opacity: 0.55;
    }
  }

  .labels {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: row;
    align-items: baseline;
    gap: 0.35rem;
    overflow: hidden;
    white-space: nowrap;
  }

  .primary {
    font-size: 0.95rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--text);
    line-height: 1;
    flex-shrink: 0;
  }

  .primary.recording {
    color: #ff4444;
    font-family: var(--font-mono);
    letter-spacing: 0.5px;
  }

  .sep {
    color: var(--text-faint);
    font-size: 0.78rem;
    flex-shrink: 0;
  }

  .secondary {
    font-size: 0.72rem;
    color: var(--text-muted);
    line-height: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .progress-ring {
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: conic-gradient(
      var(--accent) var(--progress, 0%),
      var(--border-strong) 0
    );
    position: relative;
  }

  .progress-ring::after {
    content: "";
    position: absolute;
    inset: 3px;
    border-radius: 50%;
    background: var(--surface);
  }

  .error-strip {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    background: var(--danger-bg);
    border: 1px solid var(--danger-border);
    color: var(--danger);
    padding: 0.3rem 0.55rem;
    border-radius: var(--radius);
    font-size: 0.72rem;
  }

  .error-msg {
    margin: 0;
    flex: 1;
    word-break: break-word;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .retry-btn {
    appearance: none;
    background: transparent;
    border: 1px solid currentColor;
    color: inherit;
    padding: 0.15rem 0.55rem;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 0.68rem;
    flex-shrink: 0;
  }

  .retry-btn:hover {
    background: var(--danger-bg);
  }
</style>
