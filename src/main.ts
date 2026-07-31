import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import './styles.css';

// ── Types ────────────────────────────────────────────────────
interface FullStatus {
  running: boolean;
  profile: string;
  server: string | null;
  uptime_secs: number;
  pid: number | null;
  traffic_up: number;
  traffic_down: number;
  total_up: number;
  total_down: number;
  log_lines: string[];
}

interface Profile {
  name: string;
  is_active: boolean;
  server?: string;
  ss_server?: string;
  version?: number;
  ss_method?: string;
  server_addr?: string;
  ss_server_addr?: string;
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
const btnSettings = document.getElementById('btn-main-settings')!;
const btnLog = document.getElementById('btn-main-log')!;
const btnRefreshLog = document.getElementById('btn-refresh-log')!;
const btnBackFromLog = document.getElementById('btn-back-from-log')!;
const logContent = document.getElementById('log-content')!;
const mainProfileSelect = document.getElementById('main-profile-select') as HTMLSelectElement;
const logView = document.getElementById('log-view')!;
const container = document.querySelector('.container') as HTMLElement;

// ── Helpers ──────────────────────────────────────────────────
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

function formatSpeed(bytesPerSec: number): string {
  return `${formatBytes(bytesPerSec)}/s`;
}

function formatUptime(secs: number): string {
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

// ── Server address display ───────────────────────────────────
function formatAddress(server: string | null): string {
  if (!server) return '';
  // server is "host:port" — just show it as-is
  return server;
}

// ── Profiles ─────────────────────────────────────────────────
async function loadProfiles() {
  try {
    const profiles = await invoke<Profile[]>('get_profiles');
    mainProfileSelect.innerHTML = '';
    profiles.forEach(p => {
      const opt = document.createElement('option');
      opt.value = p.name;
      opt.textContent = p.name;
      if (p.is_active) opt.selected = true;
      mainProfileSelect.appendChild(opt);
    });
  } catch (e) {
    console.error('Failed to load profiles:', e);
  }
}

// ── Auto-ping ────────────────────────────────────────────────
let pingRunning = false;
let pingTimer: ReturnType<typeof setInterval> | null = null;

async function doPing() {
  if (pingRunning) return;
  try {
    const result = await invoke<string>('real_ping');
    pingValue.textContent = result === 'timeout' ? 'timeout' : `${result} ms`;
  } catch {
    pingValue.textContent = '-';
  }
}

function startPingLoop() {
  stopPingLoop();
  doPing();
  pingTimer = setInterval(doPing, 2000);
}

function stopPingLoop() {
  if (pingTimer) clearInterval(pingTimer);
  pingTimer = null;
  pingRunning = false;
}

// ── Status update ────────────────────────────────────────────
let lastPid: number | null = null;

async function updateStatus() {
  try {
    const s = await invoke<FullStatus>('get_full_status');

    // Update status display
    statusText.textContent = s.running ? 'Connected' : 'Disconnected';
    statusDot.classList.toggle('connected', s.running);

    // Address in status card
    statusAddress.textContent = s.running && s.server ? formatAddress(s.server) : '';

    // Ping — show last result or '-' if stopped
    if (!s.running) {
      pingValue.textContent = '-';
    }

    // Uptime
    uptimeValue.textContent = s.running ? formatUptime(s.uptime_secs) : '-';

    // Current speed
    trafficValue.textContent = s.running
      ? `↑ ${formatSpeed(s.traffic_up)}  ↓ ${formatSpeed(s.traffic_down)}`
      : '↑ 0 B/s  ↓ 0 B/s';

    // Total traffic
    totalTrafficValue.textContent = s.running
      ? `↑ ${formatBytes(s.total_up)}  ↓ ${formatBytes(s.total_down)}`
      : '↑ 0 B  ↓ 0 B';

    // Buttons
    btnStart.disabled = s.running;
    btnStop.disabled = !s.running;

    // PID changed → restart ping
    if (s.running && s.pid !== lastPid) {
      startPingLoop();
    } else if (!s.running) {
      stopPingLoop();
    }
    lastPid = s.pid;

    // Auto-clear message after reconnect
    if (s.running) clearMessage();
  } catch {
    // silent
  }
}

// ── Log view ─────────────────────────────────────────────────
function showLogView() {
  container.style.display = 'none';
  logView.style.display = 'block';
  refreshLog();
}

function hideLogView() {
  logView.style.display = 'none';
  container.style.display = 'block';
}

async function refreshLog() {
  try {
    const s = await invoke<FullStatus>('get_full_status');
    logContent.textContent = s.log_lines.join('\n');
    logContent.scrollTop = logContent.scrollHeight;
  } catch {
    logContent.textContent = 'Failed to load log.';
  }
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
  const profile = mainProfileSelect.value;
  showMessage('Starting...', false);
  try {
    await invoke('start_proxy', { profile });
    showMessage('Started successfully');
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
    uptimeValue.textContent = '-';
    trafficValue.textContent = '↑ 0 B/s  ↓ 0 B/s';
    totalTrafficValue.textContent = '↑ 0 B  ↓ 0 B';
  } catch (e: any) {
    showMessage(String(e), true);
  }
});

btnSettings.addEventListener('click', async () => {
  try {
    await invoke('open_settings_window');
  } catch (e) {
    console.error('Failed to open settings:', e);
  }
});

btnLog.addEventListener('click', showLogView);
btnBackFromLog.addEventListener('click', hideLogView);
btnRefreshLog.addEventListener('click', refreshLog);

mainProfileSelect.addEventListener('change', async () => {
  const name = mainProfileSelect.value;
  try {
    await invoke('switch_profile', { name });
    clearMessage();
  } catch (e) {
    showMessage(`Failed: ${e}`, true);
  }
});

// ── Init ─────────────────────────────────────────────────────
async function init() {
  await loadProfiles();
  await updateStatus();
}

init();

// 1-second status timer
setInterval(updateStatus, 1000);
