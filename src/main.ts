import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './styles.css';

// ── Types ────────────────────────────────────────────────────
interface FullStatus {
  running: boolean;
  mode: string;
  server: string | null;
  uptime_secs: number;
  pid: number | null;
  traffic_up: number;
  traffic_down: number;
  total_up: number;
  total_down: number;
  log_lines: string[];
}

// ── Elements ─────────────────────────────────────────────────
const statusDot = document.getElementById('status-dot')!;
const statusText = document.getElementById('status-text')!;
const statusAddress = document.getElementById('status-address')!;
const pingValue = document.getElementById('ping-value')!;
const uptimeValue = document.getElementById('uptime-value')!;
const trafficValue = document.getElementById('traffic-value')!;
const totalTrafficValue = document.getElementById('total-traffic-value')!;
const message = document.getElementById('message')!;
const btnStart = document.getElementById('btn-start') as HTMLButtonElement;
const btnStop = document.getElementById('btn-stop') as HTMLButtonElement;
const btnLog = document.getElementById('btn-main-log')!;
const profileSelector = document.getElementById('profile-selector') as HTMLSelectElement;

// Views
const mainView = document.getElementById('main-view')!;
const logView = document.getElementById('log-view')!;
const logContent = document.getElementById('log-content')!;
const btnRefreshLog = document.getElementById('btn-refresh-log')!;
const btnBackFromLog = document.getElementById('btn-back-from-log')!;

// ── Helpers ──────────────────────────────────────────────────
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

function formatSpeed(bps: number): string {
  return `${formatBytes(bps)}/s`;
}

function formatUptime(secs: number): string {
  if (!secs || secs < 1) return '-';
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return `${h}h ${m}m`;
}

function showMessage(msg: string, isError = false) {
  message.textContent = msg;
  message.className = `message ${isError ? 'error' : 'success'}`;
}

function clearMessage() {
  message.textContent = '';
  message.className = 'message';
}

// ── Views ────────────────────────────────────────────────────
function showView(view: 'main' | 'log') {
  mainView.style.display = view === 'main' ? 'block' : 'none';
  logView.style.display = view === 'log' ? 'block' : 'none';
  if (view === 'log') refreshLog();
}

// ── Auto-ping ────────────────────────────────────────────────
let pingTimer: ReturnType<typeof setInterval> | null = null;

async function doPing() {
  try {
    const result = await invoke<string>('real_ping');
    pingValue.textContent = result;
  } catch {
    pingValue.textContent = '-';
  }
}

function startPingLoop() {
  stopPingLoop();
  doPing();
  pingTimer = setInterval(doPing, 5000);
}

function stopPingLoop() {
  if (pingTimer) clearInterval(pingTimer);
  pingTimer = null;
}

// ── Status update (every 2s) ─────────────────────────────────
let lastPid: number | null = null;
let uptimeStartSecs: number | null = null;
let uptimeTimer: ReturnType<typeof setInterval> | null = null;

function startUptimeTimer() {
  stopUptimeTimer();
  uptimeTimer = setInterval(() => {
    if (uptimeStartSecs !== null) {
      const elapsed = uptimeStartSecs + Math.floor((Date.now() - uptimeRefresh) / 1000);
      uptimeValue.textContent = formatUptime(elapsed);
    }
  }, 1000);
}

function stopUptimeTimer() {
  if (uptimeTimer) clearInterval(uptimeTimer);
  uptimeTimer = null;
  uptimeStartSecs = null;
}

let uptimeRefresh = Date.now();

async function updateStatus() {
  try {
    const s = await invoke<FullStatus>('get_full_status');
    uptimeRefresh = Date.now();

    statusText.textContent = s.running ? 'Connected' : 'Disconnected';
    statusDot.classList.toggle('connected', s.running);
    statusAddress.textContent = s.running && s.server ? s.server : '';

    if (!s.running) pingValue.textContent = '-';

    uptimeStartSecs = s.uptime_secs;
    uptimeValue.textContent = formatUptime(s.uptime_secs);

    trafficValue.textContent = s.running
      ? `↑ ${formatSpeed(s.traffic_up)}  ↓ ${formatSpeed(s.traffic_down)}`
      : '↑ 0 B/s  ↓ 0 B/s';

    totalTrafficValue.textContent = s.running
      ? `↑ ${formatBytes(s.total_up)}  ↓ ${formatBytes(s.total_down)}`
      : '↑ 0 B  ↓ 0 B';

    btnStart.disabled = s.running;
    btnStop.disabled = !s.running;

    if (s.running && s.pid !== lastPid) {
      startPingLoop();
      startUptimeTimer();
    } else if (!s.running) {
      stopPingLoop();
      stopUptimeTimer();
    }
    lastPid = s.pid ?? null;

    if (s.running) clearMessage();
  } catch { /* silent */ }
}

// ── Log ──────────────────────────────────────────────────────
async function refreshLog() {
  try {
    const s = await invoke<FullStatus>('get_full_status');
    logContent.textContent = s.log_lines.join('\n') || 'No log available';
    logContent.scrollTop = logContent.scrollHeight;
  } catch {
    logContent.textContent = 'Failed to load log.';
  }
}

// ── Profile management ───────────────────────────────────────
async function loadProfile() {
  try {
    const profile = await invoke<string>('get_profile');
    profileSelector.value = profile;
    updateH2PresetVisibility(profile);
    if (profile.endsWith('-h2')) loadH2PresetSelection();
  } catch (e) {
    console.error('Failed to load profile:', e);
  }
}

function updateH2PresetVisibility(profile: string) {
  const h2Sel = document.getElementById('h2-preset-selector');
  if (h2Sel) h2Sel.style.display = profile.endsWith('-h2') ? 'block' : 'none';
}

async function loadH2PresetSelection() {
  try {
    const s = await invoke<{ up_mbps: number; down_mbps: number }>('get_h2_speeds');
    const dropdown = document.getElementById('h2-preset-dropdown') as HTMLSelectElement;
    if (!dropdown) return;
    const { up_mbps, down_mbps } = s;
    if (up_mbps === 4 && down_mbps === 16) dropdown.value = 'adsl';
    else if (up_mbps === 15 && down_mbps === 30) dropdown.value = '4g';
    else if (up_mbps === 40 && down_mbps === 80) dropdown.value = '5g';
    else if (up_mbps === 80 && down_mbps === 120) dropdown.value = 'max';
  } catch (e) { /* silent */ }
}

// ── Events ───────────────────────────────────────────────────
listen('proxy-log', (event: { payload: string }) => {
  if (logView.style.display !== 'none') {
    logContent.textContent += `\n${event.payload}`;
    logContent.scrollTop = logContent.scrollHeight;
  }
});

// ── Button handlers ──────────────────────────────────────────
btnStart.addEventListener('click', async () => {
  clearMessage();
  showMessage('Starting...', false);
  try {
    await invoke('start_proxy');
    showMessage('Started');
    startPingLoop();
    lastPid = null;
  } catch (e: any) {
    showMessage(String(e), true);
  }
});

btnStop.addEventListener('click', async () => {
  clearMessage();
  try {
    await invoke('stop_proxy');
    showMessage('Stopped');
    stopPingLoop();
    lastPid = null;
    pingValue.textContent = '-';
  } catch (e: any) {
    showMessage(String(e), true);
  }
});

btnLog.addEventListener('click', () => showView('log'));
btnBackFromLog.addEventListener('click', () => showView('main'));
btnRefreshLog.addEventListener('click', refreshLog);

profileSelector.addEventListener('change', async () => {
  try {
    await invoke('set_profile', { profile: profileSelector.value });
    updateH2PresetVisibility(profileSelector.value);
    if (profileSelector.value.endsWith('-h2')) loadH2PresetSelection();
    await updateStatus();
    showMessage('Profile changed', false);
  } catch (e) {
    showMessage(`Failed: ${e}`, true);
  }
});

document.getElementById('h2-preset-dropdown')?.addEventListener('change', async (e) => {
  const target = e.target as HTMLSelectElement;
  try {
    await invoke('apply_h2_preset', { name: target.value });
    showMessage('Preset applied', false);
  } catch (e) {
    showMessage(`Failed: ${e}`, true);
  }
});

// ── Init ─────────────────────────────────────────────────────
(async () => {
  await loadProfile();
  await updateStatus();
})();

setInterval(updateStatus, 2000);
