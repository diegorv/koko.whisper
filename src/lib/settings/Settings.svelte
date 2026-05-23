<script lang="ts">
  // Settings window. Two-pane layout: left sidebar lists categories
  // grouped by area, right pane renders the active section as a card.
  // Mirrors quick-capture's SettingsDialog shape (ADR-0002 §6).
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { onMount, onDestroy } from "svelte";

  interface AudioDevice {
    name: string;
    device_type: "Input" | "System";
    is_default: boolean;
  }

  interface SelectedDevice {
    name: string;
    device_type: "Input" | "System";
  }

  interface AppSettings {
    output_folder: string;
    mic_device: SelectedDevice | null;
    sys_device: SelectedDevice | null;
    mic_enabled: boolean;
    sys_enabled: boolean;
  }

  type SectionId =
    | "devices"
    | "folder"
    | "shortcuts"
    | "model"
    | "storage"
    | "about";

  const SECTIONS: Array<{ id: SectionId; label: string; group: string }> = [
    { id: "devices", label: "Devices", group: "Capture" },
    { id: "folder", label: "Folder", group: "Capture" },
    { id: "shortcuts", label: "Shortcuts", group: "General" },
    { id: "model", label: "Model", group: "General" },
    { id: "storage", label: "Storage", group: "Advanced" },
    { id: "about", label: "About", group: "Advanced" },
  ];

  const GROUPED = SECTIONS.reduce<
    Array<{ group: string; items: typeof SECTIONS }>
  >((acc, item) => {
    const last = acc[acc.length - 1];
    if (last && last.group === item.group) last.items.push(item);
    else acc.push({ group: item.group, items: [item] });
    return acc;
  }, []);

  const SHORTCUTS: Array<{ keys: string[]; label: string }> = [
    { keys: ["⌘", "⇧", "R"], label: "Start / stop recording" },
    { keys: ["⌘", "⇧", "H"], label: "Show / hide history" },
    { keys: ["⌘", ","], label: "Open Settings (from History)" },
    { keys: ["⌘", "Q"], label: "Quit" },
  ];

  let activeSection = $state<SectionId>("devices");

  let outputFolder = $state("");
  let devices: AudioDevice[] = $state([]);
  let micDevice: SelectedDevice | null = $state(null);
  let sysDevice: SelectedDevice | null = $state(null);
  let micEnabled = $state(true);
  let sysEnabled = $state(false);
  let audioLevels: Record<string, number> = $state({});
  let levelInterval: ReturnType<typeof setInterval> | null = null;

  let micDevices = $derived(devices.filter((d) => d.device_type === "Input"));
  let systemDevices = $derived(
    devices.filter((d) => d.device_type === "System"),
  );

  function deviceKey(d: SelectedDevice | null): string {
    if (!d) return "__default__";
    return `${d.device_type}::${d.name}`;
  }

  onMount(async () => {
    const [settings, deviceList] = await Promise.all([
      invoke<AppSettings>("get_settings"),
      invoke<AudioDevice[]>("list_audio_devices"),
    ]);

    outputFolder = settings.output_folder;
    micDevice = settings.mic_device;
    sysDevice = settings.sys_device;
    micEnabled = settings.mic_enabled;
    sysEnabled = settings.sys_enabled;
    devices = deviceList;

    levelInterval = setInterval(async () => {
      try {
        audioLevels = await invoke<Record<string, number>>("get_audio_levels");
      } catch {}
    }, 100);
  });

  onDestroy(() => {
    if (levelInterval) clearInterval(levelInterval);
  });

  function levelColor(level: number): string {
    if (level > 0.8) return "#e74c3c";
    if (level > 0.4) return "#f39c12";
    return "#2ecc71";
  }

  async function pickFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select transcription folder",
    });
    if (selected) {
      outputFolder = selected as string;
      await invoke("set_output_folder", { path: outputFolder });
    }
  }

  async function revealFolder() {
    if (!outputFolder) return;
    try {
      await revealItemInDir(outputFolder);
    } catch (e) {
      console.error("revealItemInDir failed", e);
    }
  }

  async function changeMicDevice(event: Event) {
    const select = event.target as HTMLSelectElement;
    const value = select.value;

    if (value === "__default__") {
      micDevice = null;
      await invoke("set_mic_device", { device: null });
    } else {
      const [type_, ...nameParts] = value.split("::");
      const name = nameParts.join("::");
      const device: SelectedDevice = {
        name,
        device_type: type_ as "Input" | "System",
      };
      micDevice = device;
      await invoke("set_mic_device", { device });
    }
  }

  async function changeSysDevice(event: Event) {
    const select = event.target as HTMLSelectElement;
    const value = select.value;

    if (value === "__none__") {
      sysDevice = null;
      await invoke("set_sys_device", { device: null });
    } else {
      const [type_, ...nameParts] = value.split("::");
      const name = nameParts.join("::");
      const device: SelectedDevice = {
        name,
        device_type: type_ as "Input" | "System",
      };
      sysDevice = device;
      await invoke("set_sys_device", { device });
    }
  }

  async function toggleMic() {
    micEnabled = !micEnabled;
    await invoke("set_mic_enabled", { enabled: micEnabled });
  }

  async function toggleSys() {
    sysEnabled = !sysEnabled;
    const device = await invoke<SelectedDevice | null>("set_sys_enabled", {
      enabled: sysEnabled,
    });
    sysDevice = device;
  }
</script>

<div class="settings">
  <nav class="sidebar" aria-label="Settings sections">
    <h1 class="brand">Settings</h1>
    {#each GROUPED as group}
      <div class="group">
        <div class="group-label">{group.group}</div>
        {#each group.items as item}
          <button
            type="button"
            class="nav-item"
            class:active={activeSection === item.id}
            aria-current={activeSection === item.id ? "page" : undefined}
            onclick={() => (activeSection = item.id)}
          >
            {item.label}
          </button>
        {/each}
      </div>
    {/each}
  </nav>

  <div class="details">
    {#if activeSection === "devices"}
      <section class="section">
        <h2>Devices</h2>
        <p class="lede">Audio sources captured during a recording.</p>

        <div class="device-row">
          <div class="device-row-header">
            <label for="mic-select">Microphone</label>
            <button class="toggle" class:active={micEnabled} onclick={toggleMic}>
              {micEnabled ? "Enabled" : "Disabled"}
            </button>
          </div>
          <div class:disabled={!micEnabled}>
            <select
              id="mic-select"
              class="device-select"
              value={deviceKey(micDevice)}
              onchange={changeMicDevice}
              disabled={!micEnabled}
            >
              <option value="__default__">System default microphone</option>
              {#each micDevices as device}
                <option value="Input::{device.name}">
                  {device.name}
                  {device.is_default ? " (default)" : ""}
                </option>
              {/each}
            </select>
            <div class="vu-meter">
              <div
                class="vu-meter-bar"
                style="width: {micEnabled
                  ? (audioLevels['microphone'] ?? 0) * 100
                  : 0}%; background: {levelColor(audioLevels['microphone'] ?? 0)}"
              ></div>
            </div>
          </div>
          <p class="hint">Captures your voice.</p>
        </div>

        <div class="device-row">
          <div class="device-row-header">
            <label for="sys-select">System audio</label>
            <button class="toggle" class:active={sysEnabled} onclick={toggleSys}>
              {sysEnabled ? "Enabled" : "Disabled"}
            </button>
          </div>
          <div class:disabled={!sysEnabled}>
            {#if sysEnabled && systemDevices.length > 0}
              <select
                id="sys-select"
                class="device-select"
                value={sysDevice ? deviceKey(sysDevice) : "__none__"}
                onchange={changeSysDevice}
              >
                {#each systemDevices as device}
                  <option value="System::{device.name}">{device.name}</option>
                {/each}
              </select>
              <div class="vu-meter">
                <div
                  class="vu-meter-bar"
                  style="width: {(audioLevels['system'] ?? 0) *
                    100}%; background: {levelColor(audioLevels['system'] ?? 0)}"
                ></div>
              </div>
            {/if}
          </div>
          <p class="hint">
            {#if systemDevices.length > 0}
              Captures meeting audio, video calls, and other apps via
              ScreenCaptureKit.
            {:else}
              Requires macOS 13+ and Screen Recording permission.
            {/if}
          </p>
        </div>
      </section>
    {:else if activeSection === "folder"}
      <section class="section">
        <h2>Transcription folder</h2>
        <p class="lede">Where new transcriptions are saved as Markdown.</p>
        <div class="folder-picker">
          <input
            id="folder-input"
            type="text"
            readonly
            value={outputFolder}
            aria-label="Transcription folder"
          />
          <button type="button" class="btn" onclick={pickFolder}>
            Change…
          </button>
          <button
            type="button"
            class="btn reveal"
            onclick={revealFolder}
            disabled={!outputFolder}
          >
            Reveal in Finder
          </button>
        </div>
      </section>
    {:else if activeSection === "shortcuts"}
      <section class="section">
        <h2>Shortcuts</h2>
        <p class="lede">Read-only. Editing comes later.</p>
        <dl class="shortcuts">
          {#each SHORTCUTS as s}
            <dt>
              {#each s.keys as key}<kbd>{key}</kbd>{/each}
            </dt>
            <dd>{s.label}</dd>
          {/each}
        </dl>
      </section>
    {:else if activeSection === "model"}
      <section class="section">
        <h2>Model</h2>
        <p class="lede">Whisper model used for transcription.</p>
        <dl class="kv">
          <dt>File</dt>
          <dd class="mono">ggml-large-v3-turbo-q5_0</dd>
          <dt>Language</dt>
          <dd>Portuguese (BR)</dd>
        </dl>
      </section>
    {:else if activeSection === "storage"}
      <section class="section">
        <h2>Storage</h2>
        <dl class="kv">
          <dt>Folder</dt>
          <dd class="path-row">
            <span class="path">{outputFolder || "—"}</span>
            <button
              type="button"
              class="btn reveal"
              onclick={revealFolder}
              disabled={!outputFolder}
            >
              Reveal in Finder
            </button>
          </dd>
        </dl>
      </section>
    {:else if activeSection === "about"}
      <section class="section">
        <h2>About</h2>
        <dl class="kv">
          <dt>Version</dt>
          <dd class="mono">{__BUILD_INFO__}</dd>
        </dl>
      </section>
    {/if}
  </div>
</div>

<style>
  .settings {
    display: grid;
    grid-template-columns: 200px 1fr;
    min-height: 100vh;
    background: var(--bg);
  }

  .sidebar {
    padding: 1.25rem 0.75rem 1rem;
    background: var(--surface-sunken);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .brand {
    margin: 0 0.5rem 0.75rem;
    font-size: 0.95rem;
    letter-spacing: -0.01em;
    font-weight: 600;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .group-label {
    padding: 0.4rem 0.5rem 0.25rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  .nav-item {
    appearance: none;
    background: transparent;
    border: 0;
    text-align: left;
    padding: 0.4rem 0.6rem;
    font: inherit;
    font-size: 0.85rem;
    color: inherit;
    border-radius: var(--radius);
    cursor: pointer;
    transition: background 80ms ease;
  }

  .nav-item:hover {
    background: var(--surface-raised);
  }

  .nav-item.active {
    background: var(--accent-bg);
    color: var(--accent);
    font-weight: 500;
  }

  .details {
    padding: 1.5rem 1.75rem 2rem;
    overflow-y: auto;
    max-width: 640px;
  }

  .section {
    padding: 1rem 1.1rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
  }

  .section h2 {
    margin: 0 0 0.35rem;
    font-size: 0.95rem;
    font-weight: 600;
  }

  .lede {
    margin: 0 0 1rem;
    color: var(--text-muted);
    font-size: 0.82rem;
  }

  .device-row {
    margin-bottom: 1.1rem;
  }

  .device-row:last-child {
    margin-bottom: 0;
  }

  .device-row-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 6px;
  }

  .device-row-header label {
    font-size: 12px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin: 0;
  }

  .toggle {
    appearance: none;
    background: var(--surface-raised);
    border: 1px solid var(--border-strong);
    color: var(--text-muted);
    padding: 2px 10px;
    border-radius: 999px;
    cursor: pointer;
    font-size: 11px;
    transition: all 0.15s ease;
  }

  .toggle:hover {
    border-color: var(--accent-border);
  }

  .toggle.active {
    background: rgba(46, 204, 113, 0.15);
    border-color: #2ecc71;
    color: #2ecc71;
  }

  .disabled {
    opacity: 0.35;
    pointer-events: none;
  }

  .device-select {
    width: 100%;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    color: var(--text);
    padding: 6px 10px;
    border-radius: var(--radius);
    font-size: 13px;
    cursor: pointer;
  }

  .device-select:hover {
    border-color: var(--accent-border);
  }

  .vu-meter {
    height: 4px;
    background: var(--surface-sunken);
    border-radius: 2px;
    margin-top: 6px;
    overflow: hidden;
  }

  .vu-meter-bar {
    height: 100%;
    border-radius: 2px;
    transition: width 0.08s linear;
    min-width: 0;
  }

  .hint {
    font-size: 11px;
    color: var(--text-faint);
    margin: 6px 0 0 0;
  }

  .folder-picker {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .folder-picker input {
    flex: 1 1 100%;
    background: var(--surface-sunken);
    border: 1px solid var(--border-strong);
    color: var(--text);
    padding: 8px 10px;
    border-radius: var(--radius);
    font-size: 12px;
    font-family: var(--font-mono);
  }

  .btn {
    appearance: none;
    background: var(--surface-raised);
    border: 1px solid var(--border-strong);
    color: var(--text);
    padding: 6px 12px;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 12px;
    white-space: nowrap;
  }

  .btn:hover {
    background: var(--accent-bg);
    border-color: var(--accent-border);
    color: var(--accent);
  }

  .btn.reveal {
    background: var(--accent-bg);
    border-color: var(--accent-border);
    color: var(--accent);
  }

  .btn.reveal:hover {
    background: var(--accent-bg-strong);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  dl.kv,
  dl.shortcuts {
    margin: 0;
    display: grid;
    grid-template-columns: max-content 1fr;
    column-gap: 1rem;
    row-gap: 0.55rem;
    font-size: 0.85rem;
    align-items: center;
  }

  dl.kv dt {
    text-transform: uppercase;
    font-size: 0.7rem;
    letter-spacing: 0.06em;
    font-weight: 600;
    opacity: 0.55;
  }

  dl dd {
    margin: 0;
    word-break: break-word;
  }

  .mono {
    font-family: var(--font-mono);
    font-size: 0.82rem;
  }

  .path {
    font-family: var(--font-mono);
    font-size: 0.78rem;
    word-break: break-all;
  }

  .path-row {
    display: flex;
    align-items: flex-start;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  dl.shortcuts dt {
    display: flex;
    gap: 0.15rem;
    align-items: center;
  }

  dl.shortcuts kbd {
    display: inline-block;
    min-width: 1.5em;
    text-align: center;
    padding: 0.05em 0.4em;
    font-family: inherit;
    font-size: 0.8em;
    background: var(--surface-raised);
    border: 1px solid var(--border-strong);
    border-bottom-width: 2px;
    border-radius: var(--radius-sm);
    color: inherit;
  }
</style>
