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

interface Config {
  server_address: string;
  ss_port: number;
  ss_password: string;
  stls_port: number;
  stls_password: string;
  stls_sni: string;
  socks5_port: number;
  mtu?: number;
  split_mode?: string;
  split_rules?: { pattern: string }[];
  mode: string;
  h2_port: number;
  h2_password: string;
  h2_sni: string;
  h2_insecure: boolean;
  h2_obfs: string;
  h2_obfs_password: string;
  h2_mport: string;
  h2_up_mbps: number;
  h2_down_mbps: number;
  h2_auto: boolean;
}

// ── Elements ─────────────────────────────────────────────
// Header elements
const serverSelector = document.getElementById('server-selector') as HTMLSelectElement;
const protocolTabs = document.querySelectorAll('.protocol-tabs .tab');
const btnSettingsToggle = document.getElementById('btn-settings-toggle')!;
const btnLog = document.getElementById('btn-main-log')!;

// Status elements
const statusDot = document.getElementById('status-dot')!;
const statusText = document.getElementById('status-text')!;
const statusAddress = document.getElementById('status-address')!;
const statusCard = document.querySelector('.status-card')!;

// Metrics elements
const pingValue = document.getElementById('ping-value')!;
const uptimeValue = document.getElementById('uptime-value')!;
const trafficUpValue = document.getElementById('traffic-up-value')!;
const trafficDownValue = document.getElementById('traffic-down-value')!;
const splitIndicator = document.getElementById('split-indicator')!;
const sparklineUp = document.getElementById('sparkline-up') as HTMLCanvasElement;
const sparklineDown = document.getElementById('sparkline-down') as HTMLCanvasElement;

// Controls elements
const btnStart = document.getElementById('btn-start') as HTMLButtonElement;
const btnStop = document.getElementById('btn-stop') as HTMLButtonElement;
const message = document.getElementById('message')!;

// Inline log elements
const logSection = document.getElementById('log-section')!;
const logToggle = document.getElementById('log-toggle')!;
const inlineLogContent = document.getElementById('inline-log-content')!;

// Settings panel
const settingsPanel = document.getElementById('settings-panel')!;

// Views
const mainView = document.getElementById('main-view')!;
const logView = document.getElementById('log-view')!;
const logContent = document.getElementById('log-content')!;
const btnRefreshLog = document.getElementById('btn-refresh-log')!;
const btnBackFromLog = document.getElementById('btn-back-from-log')!;

// Settings inputs
const settingSplitMode = document.getElementById('setting-split-mode') as HTMLSelectElement;
const customRulesContainer = document.getElementById('custom-rules-container')!;
const settingMtu = document.getElementById('setting-mtu') as HTMLInputElement;
const settingSplitRules = document.getElementById('setting-split-rules') as HTMLTextAreaElement;
const btnSaveSettings = document.getElementById('btn-save-settings')!;
const btnUpdateGeofiles = document.getElementById('btn-update-geofiles')!;

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

// ── Sparkline rendering ──────────────────────────────────────
const SPARKLINE_POINTS = 30;
const upHistory: number[] = [];
const downHistory: number[] = [];

// Initialize with zeros
for (let i = 0; i < SPARKLINE_POINTS; i++) {
  upHistory.push(0);
  downHistory.push(0);
}

function drawSparkline(canvas: HTMLCanvasElement, data: number[], color: string) {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  
  const width = canvas.width;
  const height = canvas.height;
  const max = Math.max(...data, 1);
  
  ctx.clearRect(0, 0, width, height);
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  
  data.forEach((value, i) => {
    const x = (i / (SPARKLINE_POINTS - 1)) * width;
    const y = height - (value / max) * height;
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  
  ctx.stroke();
}

function updateSparklines(upSpeed: number, downSpeed: number) {
  upHistory.shift();
  upHistory.push(upSpeed);
  downHistory.shift();
  downHistory.push(downSpeed);
  
  drawSparkline(sparklineUp, upHistory, '#4ade80');
  drawSparkline(sparklineDown, downHistory, '#60a5fa');
}
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

    trafficUpValue.textContent = s.running ? formatSpeed(s.traffic_up) : '0 B/s';
    trafficDownValue.textContent = s.running ? formatSpeed(s.traffic_down) : '0 B/s';
    
    // Update sparklines
    if (s.running) {
      updateSparklines(s.traffic_up, s.traffic_down);
    }

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

async function refreshInlineLog() {
  try {
    const s = await invoke<FullStatus>('get_full_status');
    inlineLogContent.textContent = s.log_lines.join('\n') || 'No log available';
    inlineLogContent.scrollTop = inlineLogContent.scrollHeight;
  } catch {
    inlineLogContent.textContent = 'Failed to load log.';
  }
}

// ── Inline log toggle ────────────────────────────────────────
logToggle.addEventListener('click', () => {
  const isExpanded = logSection.classList.toggle('expanded');
  if (isExpanded) refreshInlineLog();
});

// ── Profile management (Phase 2: Server + Protocol) ─────────
// State: current server and protocol
let currentServer = 'germany-1';
let currentProtocol: 'h2' | 'stls' = 'h2';

function getProfileName(): string {
  return `${currentServer}-${currentProtocol}`;
}

function parseProfile(profile: string): { server: string; protocol: 'h2' | 'stls' } {
  // Parse "germany-1-h2" -> { server: "germany-1", protocol: "h2" }
  const parts = profile.split('-');
  const protocol = parts[parts.length - 1] as 'h2' | 'stls';
  const server = parts.slice(0, -1).join('-');
  return { server, protocol };
}

async function loadProfile() {
  try {
    const profile = await invoke<string>('get_profile');
    const parsed = parseProfile(profile);
    currentServer = parsed.server;
    currentProtocol = parsed.protocol;
    
    // Update UI
    serverSelector.value = currentServer;
    updateProtocolTabs(currentProtocol);
    updateH2PresetVisibility(currentProtocol);
    
    if (currentProtocol === 'h2') loadH2PresetSelection();
  } catch (e) {
    console.error('Failed to load profile:', e);
  }
}

function updateProtocolTabs(protocol: 'h2' | 'stls') {
  protocolTabs.forEach(tab => {
    const tabProtocol = (tab as HTMLElement).dataset.protocol;
    tab.classList.toggle('active', tabProtocol === protocol);
  });
  
  // Update status card border color
  statusCard.classList.remove('protocol-h2', 'protocol-stls');
  statusCard.classList.add(`protocol-${protocol}`);
}

function updateH2PresetVisibility(protocol: 'h2' | 'stls') {
  const h2Sel = document.getElementById('h2-preset-selector');
  if (h2Sel) h2Sel.style.display = protocol === 'h2' ? 'block' : 'none';
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

// ── Settings panel ───────────────────────────────────────────
async function loadSettings() {
  try {
    const cfg = await invoke<Config>('get_config');
    settingSplitMode.value = cfg.split_mode || 'full';
    settingMtu.value = cfg.mtu ? String(cfg.mtu) : '';
    settingSplitRules.value = cfg.split_rules?.map(r => r.pattern).join('\n') || '';
    
    // Update split indicator
    updateSplitIndicator(cfg.split_mode || 'full');
    
    // Trigger split mode change to show/hide elements
    const mode = settingSplitMode.value;
    customRulesContainer.style.display = mode === 'custom' ? 'block' : 'none';
    btnUpdateGeofiles.style.display = mode === 'iran' ? 'inline-block' : 'none';
  } catch (e) {
    console.error('Failed to load settings:', e);
  }
}

function updateSplitIndicator(splitMode: string) {
  const isActive = splitMode !== 'full';
  splitIndicator.classList.toggle('active', isActive);
  
  let tooltipText = 'Full tunnel';
  if (splitMode === 'iran') tooltipText = 'Split tunnel: Iran Direct';
  else if (splitMode === 'custom') tooltipText = 'Split tunnel: Custom rules';
  
  splitIndicator.setAttribute('title', tooltipText);
}

// ── Settings panel toggle & split mode handling ─────────────
settingSplitMode.addEventListener('change', () => {
  const mode = settingSplitMode.value;
  customRulesContainer.style.display = mode === 'custom' ? 'block' : 'none';
  btnUpdateGeofiles.style.display = mode === 'iran' ? 'inline-block' : 'none';
  updateSplitIndicator(mode);
});

btnSettingsToggle.addEventListener('click', () => {
  const visible = settingsPanel.style.display !== 'none';
  settingsPanel.style.display = visible ? 'none' : 'block';
  if (!visible) loadSettings();
});

btnSaveSettings.addEventListener('click', async () => {
  try {
    const mtu = settingMtu.value ? parseInt(settingMtu.value, 10) : null;
    const splitMode = settingSplitMode.value;
    const splitRules = settingSplitRules.value
      .split('\n')
      .map(s => s.trim())
      .filter(s => s.length > 0);

    await invoke('update_settings', { mtu, splitMode, splitRules });
    showMessage('Settings saved', false);
  } catch (e) {
    showMessage(`Failed: ${e}`, true);
  }
});

btnUpdateGeofiles.addEventListener('click', async () => {
  try {
    showMessage('Downloading geofiles...', false);
    await invoke('update_geofiles');
    showMessage('Geofiles updated', false);
  } catch (e) {
    showMessage(`Failed: ${e}`, true);
  }
});

// ── Events ───────────────────────────────────────────────────
listen('proxy-log', (event: { payload: string }) => {
  // Update inline log if expanded
  if (logSection.classList.contains('expanded')) {
    inlineLogContent.textContent += `\n${event.payload}`;
    inlineLogContent.scrollTop = inlineLogContent.scrollHeight;
  }
  
  // Update separate log view if visible
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

// ── Server selector handler ──────────────────────────────────
serverSelector.addEventListener('change', async () => {
  currentServer = serverSelector.value;
  try {
    await invoke('set_profile', { profile: getProfileName() });
    await updateStatus();
    showMessage('Server changed', false);
  } catch (e) {
    showMessage(`Failed: ${e}`, true);
  }
});

// ── Protocol tabs handler ────────────────────────────────────
protocolTabs.forEach(tab => {
  tab.addEventListener('click', async () => {
    const protocol = (tab as HTMLElement).dataset.protocol as 'h2' | 'stls';
    if (protocol === currentProtocol) return;
    
    currentProtocol = protocol;
    updateProtocolTabs(protocol);
    updateH2PresetVisibility(protocol);
    
    try {
      await invoke('set_profile', { profile: getProfileName() });
      if (protocol === 'h2') loadH2PresetSelection();
      await updateStatus();
      showMessage('Protocol changed', false);
    } catch (e) {
      showMessage(`Failed: ${e}`, true);
    }
  });
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
