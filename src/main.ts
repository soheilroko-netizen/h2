import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
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
const serverSelectorWrapper = document.getElementById('server-selector-wrapper')!;
const serverSelectorTrigger = document.getElementById('server-selector-trigger')!;
const serverSelectorFlag = document.getElementById('server-selector-flag')!;
const serverSelectorText = document.getElementById('server-selector-text')!;
const serverSelectorOptions = document.getElementById('server-selector-options')!;
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
const trafficUpValue = document.getElementById('traffic-up-value')!;
const trafficDownValue = document.getElementById('traffic-down-value')!;
const splitIndicator = document.getElementById('split-indicator')!;
const sparklineUp = document.getElementById('sparkline-up') as HTMLCanvasElement;
const sparklineDown = document.getElementById('sparkline-down') as HTMLCanvasElement;

// Controls elements
const btnStart = document.getElementById('btn-start') as HTMLButtonElement;
const btnStartText = document.getElementById('btn-start-text')!;
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
function getServerFlag(server: string): string {
  if (server.indexOf('germany') !== -1 || server.indexOf('187.127.83.147') !== -1) {
    return '<img src="https://flagcdn.com/16x12/de.png" style="margin-right:4px;vertical-align:middle;" alt="DE" />';
  }
  if (server.indexOf('finland') !== -1 || server.indexOf('62.238.60.136') !== -1) {
    return '<img src="https://flagcdn.com/16x12/fi.png" style="margin-right:4px;vertical-align:middle;" alt="FI" />';
  }
  return '';
}

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
    
    // Mark that we have a ping response
    hasPingResponse = true;
    
    // Update ping bars based on latency
    const pingMs = parseInt(result.replace('ms', ''));
    const bars = document.querySelectorAll('.ping-bar');
    
    bars.forEach(bar => {
      const threshold = parseInt((bar as HTMLElement).dataset.threshold || '0');
      if (pingMs >= threshold) {
        bar.classList.add('active');
      } else {
        bar.classList.remove('active');
      }
    });

    // Update triangle for 300+ ms
    const triangle = document.getElementById('ping-triangle');
    if (triangle) {
      triangle.classList.toggle('active', pingMs >= 300);
    }
  } catch {
    pingValue.textContent = '-';
    hasPingResponse = false;
    // Clear all bars on error
    document.querySelectorAll('.ping-bar').forEach(bar => bar.classList.remove('active'));
    const triangle = document.getElementById('ping-triangle');
    if (triangle) triangle.classList.remove('active');
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
      btnStartText.textContent = formatUptime(elapsed);
    }
  }, 1000);
}

function stopUptimeTimer() {
  if (uptimeTimer) clearInterval(uptimeTimer);
  uptimeTimer = null;
  uptimeStartSecs = null;
  btnStartText.textContent = 'Start';
}

let uptimeRefresh = Date.now();

// ── State: connection status tracking ───────────────────────
let isConnecting = false;
let hasPingResponse = false;

async function updateStatus() {
  try {
    const s = await invoke<FullStatus>('get_full_status');
    uptimeRefresh = Date.now();

    // Handle connecting state
    if (s.running && !hasPingResponse) {
      statusText.textContent = 'Connecting...';
      statusDot.classList.remove('connected');
      statusDot.style.background = 'var(--warning)';
    } else if (s.running && hasPingResponse) {
      statusText.textContent = 'Connected';
      statusDot.classList.add('connected');
      statusDot.style.background = '';
    } else {
      statusText.textContent = 'Disconnected';
      statusDot.classList.remove('connected');
      statusDot.style.background = '';
      hasPingResponse = false;
    }
    
    statusAddress.textContent = s.running && s.server ? s.server : '';
    if (s.running && s.server) {
      statusAddress.innerHTML = getServerFlag(s.server) + s.server;
    } else {
      statusAddress.textContent = '';
    }

    // TCP/UDP indicator
    const protocolIndicator = document.getElementById('protocol-indicator');
    if (s.running && protocolIndicator) {
      const isH2 = s.mode === 'hysteria2';
      protocolIndicator.textContent = isH2 ? 'UDP' : 'TCP';
      protocolIndicator.style.display = 'inline-block';
    } else if (protocolIndicator) {
      protocolIndicator.style.display = 'none';
    }

    if (!s.running) pingValue.textContent = '-';

    uptimeStartSecs = s.uptime_secs;
    btnStartText.textContent = s.running ? formatUptime(s.uptime_secs) : 'Start';

    // Toggle connected class on start button
    btnStart.classList.toggle('connected', s.running);

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
    updateServerSelectorUI(currentServer);
    updateProtocolTabs(currentProtocol);
    updateH2PresetVisibility(currentProtocol);
    
    if (currentProtocol === 'h2') loadH2PresetSelection();
  } catch (e) {
    console.error('Failed to load profile:', e);
  }
}

function updateServerSelectorUI(server: string) {
  const flagMap: Record<string, string> = {
    'germany-1': 'de',
    'finland-1': 'fi'
  };
  const displayMap: Record<string, string> = {
    'germany-1': 'Germany 1',
    'finland-1': 'Finland 1'
  };
  
  const flag = flagMap[server] || 'de';
  const display = displayMap[server] || 'Germany 1';
  
  serverSelectorFlag.innerHTML = `<img src="https://flagcdn.com/16x12/${flag}.png" alt="${flag.toUpperCase()}" />`;
  serverSelectorText.textContent = display;
  
  // Update active option
  serverSelectorOptions.querySelectorAll('.custom-select-option').forEach(opt => {
    opt.classList.toggle('active', opt.dataset.value === server);
  });
}

// ── Server selector handler ──────────────────────────────────
serverSelectorTrigger.addEventListener('click', () => {
  serverSelectorWrapper.classList.toggle('open');
});

// Close on click outside
document.addEventListener('click', (e) => {
  if (!serverSelectorWrapper.contains(e.target as Node)) {
    serverSelectorWrapper.classList.remove('open');
  }
});

serverSelectorOptions.querySelectorAll('.custom-select-option').forEach(opt => {
  opt.addEventListener('click', async () => {
    currentServer = opt.dataset.value || 'germany-1';
    serverSelectorWrapper.classList.remove('open');
    updateServerSelectorUI(currentServer);
    
    try {
      await invoke('set_profile', { profile: getProfileName() });
      await updateStatus();
      showMessage('Server changed', false);
    } catch (e) {
      showMessage(`Failed: ${e}`, true);
    }
  });
});

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
    const cards = document.querySelectorAll('.h2-preset-card');
    if (!cards.length) return;
    const { up_mbps, down_mbps } = s;
    
    // Remove active class from all cards
    cards.forEach(card => card.classList.remove('active'));
    
    // Set active card based on speeds
    let activePreset = '5g'; // default
    if (up_mbps === 4 && down_mbps === 16) activePreset = 'adsl';
    else if (up_mbps === 15 && down_mbps === 30) activePreset = '4g';
    else if (up_mbps === 40 && down_mbps === 80) activePreset = '5g';
    else if (up_mbps === 80 && down_mbps === 120) activePreset = 'max';
    
    const activeCard = document.querySelector(`.h2-preset-card[data-preset="${activePreset}"]`);
    if (activeCard) activeCard.classList.add('active');
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

btnSettingsToggle.addEventListener('click', async () => {
  const visible = settingsPanel.style.display !== 'none';
  settingsPanel.style.display = visible ? 'none' : 'block';
  
  // Resize window
  const appWindow = getCurrentWindow();
  if (visible) {
    await appWindow.setSize(new LogicalSize(500, 680));
  } else {
    await appWindow.setSize(new LogicalSize(500, 900));
    loadSettings();
  }
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

// ── H2 Preset Cards ──────────────────────────────────────────
document.querySelectorAll('.h2-preset-card').forEach(card => {
  card.addEventListener('click', async (e) => {
    const target = e.currentTarget as HTMLElement;
    const preset = target.dataset.preset;
    if (!preset) return;
    
    // Update active state
    document.querySelectorAll('.h2-preset-card').forEach(c => c.classList.remove('active'));
    target.classList.add('active');
    
    try {
      await invoke('apply_h2_preset', { name: preset });
      showMessage('Preset applied', false);
    } catch (e) {
      showMessage(`Failed: ${e}`, true);
    }
  });
});

// ── Init ─────────────────────────────────────────────────────
(async () => {
  await loadProfile();
  await updateStatus();
})();

setInterval(updateStatus, 2000);
