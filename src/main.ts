import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';

interface FullStatus {
  running: boolean;
  uptime: number;
  traffic_up: number;
  traffic_down: number;
  total_up: number;
  total_down: number;
}

interface ProfileStore {
  profiles: { name: string }[];
  active_profile: string;
}

let pingInterval: ReturnType<typeof setInterval> | null = null;

function formatBytes(b: number): string {
  if (b === 0) return '0 B';
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  if (b < 1024 * 1024 * 1024) return `${(b / (1024 * 1024)).toFixed(1)} MB`;
  return `${(b / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatBytesSpeed(b: number): string {
  if (b === 0) return '0 B/s';
  if (b < 1024) return `${b} B/s`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB/s`;
  if (b < 1024 * 1024 * 1024) return `${(b / (1024 * 1024)).toFixed(1)} MB/s`;
  return `${(b / (1024 * 1024 * 1024)).toFixed(2)} GB/s`;
}

async function setWindowSize(w: number, h: number) {
  try {
    await getCurrentWindow().setSize(new LogicalSize(w, h));
  } catch { /* ignore */ }
}

// ── Status ──────────────────────────────────────────────────

async function updateStatus() {
  try {
    const status = await invoke<FullStatus>('get_full_status');
    const statusDot = document.getElementById('status-dot')!;
    const statusText = document.getElementById('status-text')!;
    const btnStart = document.getElementById('btn-start') as HTMLButtonElement;
    const btnStop = document.getElementById('btn-stop') as HTMLButtonElement;

    if (status.running) {
      statusDot.classList.add('connected');
      statusText.textContent = 'Connected';
      btnStart.disabled = true;
      btnStop.disabled = false;

      document.getElementById('traffic-value')!.textContent =
        `↑ ${formatBytesSpeed(status.traffic_up)}  ↓ ${formatBytesSpeed(status.traffic_down)}`;
      document.getElementById('total-traffic-value')!.textContent =
        `↑ ${formatBytes(status.total_up)}  ↓ ${formatBytes(status.total_down)}`;

      const uptimeEl = document.getElementById('uptime-value')!;
      if (status.uptime === 0) {
        uptimeEl.textContent = '-';
      } else {
        const h = Math.floor(status.uptime / 3600);
        const m = Math.floor((status.uptime % 3600) / 60);
        const s = status.uptime % 60;
        const pad = (n: number) => n.toString().padStart(2, '0');
        uptimeEl.textContent = h > 0
          ? `${h}:${pad(m)}:${pad(s)}`
          : `${m}:${pad(s)}`;
      }

      // Start auto-ping if not running
      if (!pingInterval) startAutoPing();
    } else {
      statusDot.classList.remove('connected');
      statusText.textContent = 'Disconnected';
      btnStart.disabled = false;
      btnStop.disabled = true;

      document.getElementById('traffic-value')!.textContent = '↑ 0 B/s  ↓ 0 B/s';
      document.getElementById('total-traffic-value')!.textContent = '↑ 0 B  ↓ 0 B';
      document.getElementById('uptime-value')!.textContent = '-';

      stopAutoPing();
    }
  } catch (err) {
    showMessage('Error checking status: ' + err, 'error');
  }
}

// ── Auto-ping ───────────────────────────────────────────────

async function doPing() {
  const pingEl = document.getElementById('ping-value')!;
  try {
    const ms = await invoke<string>('real_ping');
    pingEl.textContent = ms;
  } catch {
    pingEl.textContent = 'TIMEOUT';
  }
}

function startAutoPing() {
  if (pingInterval) return;
  doPing();
  pingInterval = setInterval(doPing, 3000);
}

function stopAutoPing() {
  if (pingInterval) {
    clearInterval(pingInterval);
    pingInterval = null;
  }
  document.getElementById('ping-value')!.textContent = '-';
}

// ── Server info ─────────────────────────────────────────────

async function updateServerInfo() {
  try {
    const config = await invoke<{ server_address: string; stls_port: number }>('get_config');
    document.getElementById('server-value')!.textContent = `${config.server_address}:${config.stls_port}`;
  } catch { /* ignore */ }
}

// ── Controls ────────────────────────────────────────────────

async function startProxy() {
  try {
    const msg = await invoke<string>('start_proxy');
    showMessage(msg, 'success');
    await updateStatus();
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
  setTimeout(() => { msgEl.textContent = ''; msgEl.className = 'message'; }, 5000);
}

// ── Main view profile selector ─────────────────────────────

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
  } catch { /* ignore */ }
}

async function mainProfileChanged() {
  const select = document.getElementById('main-profile-select') as HTMLSelectElement;
  const name = select.value;
  if (!name) return;
  try {
    await invoke('switch_profile_stop', { name });
    updateServerInfo();
    updateStatus();
    showMessage(`Switched to '${name}'`, 'success');
  } catch (err) {
    showMessage('Switch failed: ' + err, 'error');
  }
}

// ── Log viewer ─────────────────────────────────────────────

async function refreshLog() {
  try {
    const log = await invoke<string>('get_log');
    document.getElementById('log-content')!.textContent = log;
  } catch (err) {
    document.getElementById('log-content')!.textContent = 'Error: ' + err;
  }
}

function showLogView() {
  document.getElementById('main-view')!.style.display = 'none';
  document.getElementById('log-view')!.style.display = 'block';
  setWindowSize(560, 500);
  refreshLog();
}

function showMainView() {
  document.getElementById('main-view')!.style.display = 'block';
  document.getElementById('log-view')!.style.display = 'none';
  setWindowSize(500, 560);
  updateServerInfo();
  loadMainProfiles();
}

// ── Init ───────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('btn-start')?.addEventListener('click', startProxy);
  document.getElementById('btn-stop')?.addEventListener('click', stopProxy);
  document.getElementById('btn-main-settings')?.addEventListener('click', async () => {
    try { await invoke('open_settings_window'); } catch (err) { showMessage('Settings: ' + err, 'error'); }
  });
  document.getElementById('btn-main-log')?.addEventListener('click', showLogView);
  document.getElementById('btn-back-from-log')?.addEventListener('click', showMainView);
  document.getElementById('btn-refresh-log')?.addEventListener('click', refreshLog);
  document.getElementById('main-profile-select')?.addEventListener('change', mainProfileChanged);

  setWindowSize(500, 560);
  updateServerInfo();
  loadMainProfiles();
  updateStatus();
  setInterval(updateStatus, 3000);
});
