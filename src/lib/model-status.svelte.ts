// Reactive snapshot of the Whisper model boot lifecycle. The boot
// sequence (download + WhisperContext init) runs in Rust `setup()`;
// every window calls `subscribe()` from its `onMount` to:
//   1. Read the current snapshot via `get_model_status` (handles the
//      case where the window opens AFTER the boot task finished — the
//      transition event for "ready" is otherwise lost).
//   2. Listen for `model-status` events for subsequent transitions.
//
// Returns a stop function the caller invokes from `onDestroy`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ModelStatus =
  | { kind: "unchecked" }
  | { kind: "downloading"; progress: number }
  | { kind: "ready" }
  | { kind: "error"; message: string };

export function createModelStatus() {
  let status = $state<ModelStatus>({ kind: "unchecked" });

  async function subscribe(): Promise<() => void> {
    const unlisten = await listen<ModelStatus>("model-status", (event) => {
      status = event.payload;
    });
    try {
      status = await invoke<ModelStatus>("get_model_status");
    } catch (err) {
      console.error("get_model_status failed", err);
    }
    return unlisten;
  }

  return {
    get current() {
      return status;
    },
    subscribe,
  };
}
