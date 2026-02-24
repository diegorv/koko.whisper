<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
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

  interface Props {
    onBack: () => void;
  }

  let { onBack }: Props = $props();
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
      title: "Selecionar pasta de transcricoes",
    });
    if (selected) {
      outputFolder = selected as string;
      await invoke("set_output_folder", { path: outputFolder });
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
  <div class="header">
    <button class="back-btn" onclick={onBack}>← Voltar</button>
    <h2>Configuracoes</h2>
  </div>

  <div class="setting-row">
    <div class="track-header">
      <label for="mic-select">Microfone</label>
      <button class="toggle" class:active={micEnabled} onclick={toggleMic}>
        {micEnabled ? "Ativado" : "Desativado"}
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
        <option value="__default__">Microfone padrao do sistema</option>
        {#each micDevices as device}
          <option value="Input::{device.name}">
            {device.name}
            {device.is_default ? " (padrao)" : ""}
          </option>
        {/each}
      </select>
      <div class="vu-meter">
        <div
          class="vu-meter-bar"
          style="width: {micEnabled ? (audioLevels['microphone'] ?? 0) * 100 : 0}%; background: {levelColor(audioLevels['microphone'] ?? 0)}"
        ></div>
      </div>
    </div>
    <p class="hint">Grava a sua voz</p>
  </div>

  <div class="setting-row">
    <div class="track-header">
      <label for="sys-select">Audio do sistema</label>
      <button class="toggle" class:active={sysEnabled} onclick={toggleSys}>
        {sysEnabled ? "Ativado" : "Desativado"}
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
            <option value="System::{device.name}">
              {device.name}
            </option>
          {/each}
        </select>
        <div class="vu-meter">
          <div
            class="vu-meter-bar"
            style="width: {(audioLevels['system'] ?? 0) * 100}%; background: {levelColor(audioLevels['system'] ?? 0)}"
          ></div>
        </div>
      {/if}
    </div>
    <p class="hint">
      {#if systemDevices.length > 0}
        Grava o audio de reunioes, videos, etc. via ScreenCaptureKit
      {:else}
        Requer macOS 13+ e permissao de Gravacao de Tela
      {/if}
    </p>
  </div>

  <div class="setting-row">
    <label for="folder-input">Pasta de transcricoes</label>
    <div class="folder-picker">
      <input id="folder-input" type="text" readonly value={outputFolder} />
      <button onclick={pickFolder}>Alterar</button>
    </div>
  </div>

  <div class="setting-row">
    <label>Modelo</label>
    <p class="info">ggml-large-v3-turbo-q5_0</p>
  </div>

  <div class="setting-row">
    <label>Atalho</label>
    <p class="info">Cmd+Shift+R</p>
  </div>
</div>

<style>
  .settings {
    padding: 8px 0;
  }

  .header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 24px;
  }

  .header h2 {
    font-size: 16px;
    margin: 0;
  }

  .back-btn {
    background: none;
    border: 1px solid #444;
    color: #ccc;
    padding: 4px 12px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
  }

  .back-btn:hover {
    border-color: #666;
  }

  .setting-row {
    margin-bottom: 16px;
  }

  .track-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 6px;
  }

  .track-header label {
    margin-bottom: 0;
  }

  .toggle {
    background: #333;
    border: 1px solid #444;
    color: #888;
    padding: 2px 10px;
    border-radius: 10px;
    cursor: pointer;
    font-size: 11px;
    transition: all 0.15s;
  }

  .toggle:hover {
    border-color: #555;
  }

  .toggle.active {
    background: #1a3a1a;
    border-color: #2ecc71;
    color: #2ecc71;
  }

  .disabled {
    opacity: 0.35;
    pointer-events: none;
  }

  .setting-row label {
    display: block;
    font-size: 12px;
    color: #666;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 6px;
  }

  .device-select {
    width: 100%;
    background: #222;
    border: 1px solid #333;
    color: #ccc;
    padding: 8px 10px;
    border-radius: 6px;
    font-size: 13px;
    cursor: pointer;
  }

  .device-select:hover {
    border-color: #555;
  }

  .vu-meter {
    height: 4px;
    background: #1a1a1a;
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
    color: #555;
    margin: 6px 0 0 0;
  }

  .folder-picker {
    display: flex;
    gap: 8px;
  }

  .folder-picker input {
    flex: 1;
    background: #222;
    border: 1px solid #333;
    color: #ccc;
    padding: 8px 10px;
    border-radius: 6px;
    font-size: 12px;
    font-family: monospace;
  }

  .folder-picker button {
    background: #333;
    border: 1px solid #444;
    color: #ccc;
    padding: 8px 14px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
    white-space: nowrap;
  }

  .folder-picker button:hover {
    background: #444;
  }

  .info {
    font-size: 13px;
    color: #aaa;
    margin: 0;
    font-family: monospace;
  }
</style>
