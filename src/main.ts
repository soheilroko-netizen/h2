import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';

interface Config {
  server_address: string;
  ss_port: number;
  ss_password: string;
  stls_port: number;
  stls_password: string;
  stls_sni: string;
  socks5_port: number;
  mtu: number | null;
  split_rules: SplitRule[];
}

interface SplitRule {
  pattern: string;
}

interface Profile {
  name: string;
  config: Config;
}

interface ProfileStore {
  profiles: Profile[];
  active_profile: string;
}

const MAIN_W = 500,
  MAIN_H = 560;
const SETTINGS_W = 560,
  SETTINGS_H = 720;

async function setWindowSize(w: number, h: number) {
  try {
    await getCurrentWindow().setSize({ type: 'Logical', width: w, height: h });
  } catch {
    /* ignore */
  }
}

async function showMainView() {
  document.getElementById('main-view')!.style.display = 'block';
  document.getElementById('settings-view')!.style.display = 'none';
  await setWindowSize(MAIN_W, MAIN_H);
  updateServerInfo();
  loadMainProfiles();
}

async function showSettingsView() {
  document.getElementById('main-view')!.style.display = 'none';
  document.getElementById('settings-view')!.style.display = 'block';
  await setWindowSize(SETTINGS_W, SETTINGS_H);
  loadProfiles();
}

async function updateServerInfo() {
  try {
    const config = await invoke<Config>('get_config');
    document.getElementById('server-value')!.textContent = `${config.server_address}:${config.stls_port}`;
  } catch {
    /* ignore */
  }
}

async function doPing() {
  try {
    const pingEl = document.getElementById('ping-value')!;
    pingEl.textContent = 'Pinging...';
    const ms = await invoke<string>('real_ping');
    pingEl.textContent = ms;
  } catch (err) {
    document.getElementById('ping-value')!.textContent = 'TIMEOUT';
  }
}

async function updateUptime() {
  try {
    const secs = await invoke<number>('get_uptime');
    const el = document.getElementById('uptime-value')!;
    if (secs === 0) {
      el.textContent = '-';
      return;
    }
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    const pad = (n: number) => n.toString().padStart(2, '0');
    if (h > 0) {
      el.textContent = `${h}:${pad(m)}:${pad(s)}`;
    } else {
      el.textContent = `${m}:${pad(s)}`;
    }
  } catch {
    /* ignore */
  }
}

function formatBytes(b: number): string {
  if (b === 0) return '0';
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  if (b < 1024 * 1024 * 1024) return `${(b / (1024 * 1024)).toFixed(1)} MB`;
  return `${(b / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

async function updateTraffic() {
  const el = document.getElementById('traffic-value')!;
  try {
    const raw = await invoke<string>('get_traffic');
    const v = JSON.parse(raw);
    const up = formatBytes(v.up);
    const down = formatBytes(v.down);
    el.textContent = `↑ ${up}  ↓ ${down}`;
  } catch {
    el.textContent = '↑ 0  ↓ 0';
  }
}

async function updateTotalTraffic() {
  const el = document.getElementById('total-traffic-value')!;
  try {
    const raw = await invoke<string>('get_total_traffic');
    const v = JSON.parse(raw);
    const up = formatBytes(v.up);
    const down = formatBytes(v.down);
    el.textContent = `↑ ${up}  ↓ ${down}`;
  } catch {
    el.textContent = '↑ 0  ↓ 0';
  }
}

async function updateStatus() {
  try {
    const isRunning = await invoke<boolean>('get_status');
    const statusDot = document.getElementById('status-dot')!;
    const statusText = document.getElementById('status-text')!;
    const btnStart = document.getElementById('btn-start') as HTMLButtonElement;
    const btnStop = document.getElementById('btn-stop') as HTMLButtonElement;

    if (isRunning) {
      statusDot.classList.add('connected');
      statusText.textContent = 'Connected';
      btnStart.disabled = true;
      btnStop.disabled = false;
    } else {
      statusDot.classList.remove('connected');
      statusText.textContent = 'Disconnected';
      btnStart.disabled = false;
      btnStop.disabled = true;
    }
  } catch (err) {
    showMessage('Error checking status: ' + err, 'error');
  }
}

async function startProxy() {
  try {
    const msg = await invoke<string>('start_proxy');
    showMessage(msg, 'success');
    await updateStatus();
    // Auto ping after connect
    setTimeout(doPing, 1500);
  } catch (err) {
    showMessage('Failed to start: ' + err, 'error');
  }
}

async function stopProxy() {
  try {
    const msg = await invoke<string>('stop_proxy');
    showMessage(msg, 'success');
    await updateStatus();
  } catch (err) {
    showMessage('Failed to stop: ' + err, 'error');
  }
}

function showMessage(text: string, type: 'success' | 'error') {
  const msgEl = document.getElementById('message')!;
  msgEl.textContent = text;
  msgEl.className = `message ${type}`;
  setTimeout(() => {
    msgEl.textContent = '';
    msgEl.className = 'message';
  }, 5000);
}

// ── Main view profile selector ─────────────────────────────────

async function loadMainProfiles() {
  try {
    const store = await invoke<ProfileStore>('get_profiles');
    const select = document.getElementById('main-profile-select') as HTMLSelectElement;
    const current = select.value;
    select.innerHTML = '';
    store.profiles.forEach((p) => {
      const opt = document.createElement('option');
      opt.value = p.name;
      opt.textContent = p.name;
      select.appendChild(opt);
    });
    if (current && store.profiles.some((p) => p.name === current)) {
      select.value = current;
    }
  } catch {
    /* ignore */
  }
}

async function mainProfileChanged() {
  const select = document.getElementById('main-profile-select') as HTMLSelectElement;
  const name = select.value;
  if (!name) return;
  try {
    await invoke('switch_profile_stop', { name });
    // Re-read config for updated server info
    updateServerInfo();
    updateStatus();
    showMessage(`Switched to '${name}'`, 'success');
  } catch (err) {
    showMessage('Switch failed: ' + err, 'error');
  }
}

// ── Settings functions ─────────────────────────────────────────

async function loadProfiles() {
  try {
    const store = await invoke<ProfileStore>('get_profiles');
    const select = document.getElementById('profile-select') as HTMLSelectElement;
    select.innerHTML = '';

    store.profiles.forEach((profile) => {
      const option = document.createElement('option');
      option.value = profile.name;
      option.textContent = profile.name;
      if (profile.name === store.active_profile) {
        option.selected = true;
      }
      select.appendChild(option);
    });

    await loadConfig();
  } catch (err) {
    showSettingsMessage('Failed to load profiles: ' + err, 'error');
  }
}

async function loadConfig() {
  try {
    const config = await invoke<Config>('get_config');
    (document.getElementById('server_address') as HTMLInputElement).value =
      config.server_address;
    (document.getElementById('ss_port') as HTMLInputElement).value =
      config.ss_port.toString();
    (document.getElementById('ss_password') as HTMLInputElement).value =
      config.ss_password;
    (document.getElementById('stls_port') as HTMLInputElement).value =
      config.stls_port.toString();
    (document.getElementById('stls_password') as HTMLInputElement).value =
      config.stls_password;
    (document.getElementById('stls_sni') as HTMLInputElement).value =
      config.stls_sni;
    (document.getElementById('socks5_port') as HTMLInputElement).value =
      config.socks5_port.toString();
    (document.getElementById('mtu') as HTMLInputElement).value =
      config.mtu ? config.mtu.toString() : '';
    (document.getElementById('split_rules') as HTMLTextAreaElement).value =
      config.split_rules.map((r) => r.pattern).join('\n');
  } catch (err) {
    showSettingsMessage('Failed to load config: ' + err, 'error');
  }
}

async function saveConfig(event: Event) {
  event.preventDefault();

  const mtuRaw = (document.getElementById('mtu') as HTMLInputElement).value;
  const mtuVal = mtuRaw ? parseInt(mtuRaw) : null;
  const splitRaw = (document.getElementById('split_rules') as HTMLTextAreaElement).value;
  const splitRules = splitRaw
    .split('\n')
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
    .map((s) => ({ pattern: s }));

  const config: Config = {
    server_address: (
      document.getElementById('server_address') as HTMLInputElement
    ).value,
    ss_port: parseInt(
      (document.getElementById('ss_port') as HTMLInputElement).value
    ),
    ss_password: (
      document.getElementById('ss_password') as HTMLInputElement
    ).value,
    stls_port: parseInt(
      (document.getElementById('stls_port') as HTMLInputElement).value
    ),
    stls_password: (
      document.getElementById('stls_password') as HTMLInputElement
    ).value,
    stls_sni: (document.getElementById('stls_sni') as HTMLInputElement).value,
    socks5_port: parseInt(
      (document.getElementById('socks5_port') as HTMLInputElement).value
    ),
    mtu: mtuVal,
    split_rules: splitRules,
  };

  try {
    await invoke('save_config', { config });
    showSettingsMessage('Settings saved successfully!', 'success');
    setTimeout(() => showMainView(), 1500);
  } catch (err) {
    showSettingsMessage('Failed to save: ' + err, 'error');
  }
}

async function switchProfile() {
  const select = document.getElementById('profile-select') as HTMLSelectElement;
  const profileName = select.value;

  try {
    await invoke('switch_profile', { name: profileName });
    await loadConfig();
    showSettingsMessage('Switched to ' + profileName, 'success');
  } catch (err) {
    showSettingsMessage('Failed to switch: ' + err, 'error');
  }
}

async function newProfile() {
  const name = prompt('Enter profile name:');
  if (!name || name.trim() === '') return;

  const mtuRaw = (document.getElementById('mtu') as HTMLInputElement).value;
  const mtuVal = mtuRaw ? parseInt(mtuRaw) : null;
  const splitRaw = (document.getElementById('split_rules') as HTMLTextAreaElement).value;
  const splitRules = splitRaw
    .split('\n')
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
    .map((s) => ({ pattern: s }));

  const config: Config = {
    server_address: (
      document.getElementById('server_address') as HTMLInputElement
    ).value,
    ss_port: parseInt(
      (document.getElementById('ss_port') as HTMLInputElement).value
    ),
    ss_password: (
      document.getElementById('ss_password') as HTMLInputElement
    ).value,
    stls_port: parseInt(
      (document.getElementById('stls_port') as HTMLInputElement).value
    ),
    stls_password: (
      document.getElementById('stls_password') as HTMLInputElement
    ).value,
    stls_sni: (document.getElementById('stls_sni') as HTMLInputElement).value,
    socks5_port: parseInt(
      (document.getElementById('socks5_port') as HTMLInputElement).value
    ),
    mtu: mtuVal,
    split_rules: splitRules,
  };

  try {
    await invoke('add_profile', { name: name.trim(), config });
    await loadProfiles();
    showSettingsMessage('Profile created!', 'success');
  } catch (err) {
    showSettingsMessage('Failed to create: ' + err, 'error');
  }
}

async function deleteProfile() {
  const select = document.getElementById('profile-select') as HTMLSelectElement;
  const profileName = select.value;

  if (profileName === 'Default') {
    showSettingsMessage('Cannot delete Default profile', 'error');
    return;
  }

  if (!confirm(`Delete profile "${profileName}"?`)) return;

  try {
    await invoke('delete_profile', { name: profileName });
    await loadProfiles();
    showSettingsMessage('Profile deleted', 'success');
  } catch (err) {
    showSettingsMessage('Failed to delete: ' + err, 'error');
  }
}

// ── Log viewer ────────────────────────────────────────────────

async function showLogView() {
  document.getElementById('main-view')!.style.display = 'none';
  document.getElementById('settings-view')!.style.display = 'none';
  document.getElementById('log-view')!.style.display = 'block';
  await setWindowSize(560, 500);
  refreshLog();
}

async function refreshLog() {
  try {
    const log = await invoke<string>('get_log');
    document.getElementById('log-content')!.textContent = log;
  } catch (err) {
    document.getElementById('log-content')!.textContent = 'Error: ' + err;
  }
}

function showSettingsMessage(text: string, type: 'success' | 'error') {
  const msgEl = document.getElementById('settings-message')!;
  msgEl.textContent = text;
  msgEl.className = `message ${type}`;
  setTimeout(() => {
    msgEl.textContent = '';
    msgEl.className = 'message';
  }, 3000);
}

// ── Polling loop for traffic + uptime ──────────────────────────

let polling = false;
async function startPolling() {
  if (polling) return;
  polling = true;
  while (polling) {
    try {
      const running = await invoke<boolean>('get_status');
      if (running) {
        await updateTraffic();
        await updateTotalTraffic();
        await updateUptime();
      } else {
        document.getElementById('traffic-value')!.textContent = '↑ 0  ↓ 0';
        document.getElementById('total-traffic-value')!.textContent = '↑ 0  ↓ 0';
        document.getElementById('uptime-value')!.textContent = '-';
      }
    } catch {
      /* ignore */
    }
    await new Promise((r) => setTimeout(r, 2000));
  }
}

function stopPolling() {
  polling = false;
}

// Event listeners
document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('btn-start')?.addEventListener('click', startProxy);
  document.getElementById('btn-stop')?.addEventListener('click', stopProxy);
  document
    .getElementById('btn-main-settings')
    ?.addEventListener('click', showSettingsView);
  document
    .getElementById('btn-main-log')
    ?.addEventListener('click', showLogView);
  document.getElementById('btn-back')?.addEventListener('click', showMainView);
  document
    .getElementById('btn-back-from-log')
    ?.addEventListener('click', showMainView);
  document
    .getElementById('btn-refresh-log')
    ?.addEventListener('click', refreshLog);
  document
    .getElementById('btn-ping')
    ?.addEventListener('click', doPing);
  document
    .getElementById('main-profile-select')
    ?.addEventListener('change', mainProfileChanged);

  document
    .getElementById('settings-form')
    ?.addEventListener('submit', saveConfig);
  document
    .getElementById('profile-select')
    ?.addEventListener('change', switchProfile);
  document
    .getElementById('btn-new-profile')
    ?.addEventListener('click', newProfile);
  document
    .getElementById('btn-delete-profile')
    ?.addEventListener('click', deleteProfile);

  updateServerInfo();
  loadMainProfiles();
  updateStatus();
  setInterval(updateStatus, 2000);
  startPolling();
});
