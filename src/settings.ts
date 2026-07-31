import { invoke } from '@tauri-apps/api/core';

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

// ── Tab switching ──────────────────────────────────────────

function switchTab(tab: string) {
  document.querySelectorAll('.tab-btn').forEach(b => {
    b.classList.toggle('active', b.dataset.tab === tab);
    b.setAttribute('aria-selected', b.dataset.tab === tab ? 'true' : 'false');
  });
  document.querySelectorAll('.tab-panel').forEach(p => {
    p.hidden = p.id !== `tab-${tab}`;
  });
}

// ── Profile management ─────────────────────────────────────

async function loadProfiles() {
  try {
    const store = await invoke<ProfileStore>('get_profiles');
    const select = document.getElementById('profile-select') as HTMLSelectElement;
    select.innerHTML = '';
    store.profiles.forEach(p => {
      const opt = document.createElement('option');
      opt.value = p.name;
      opt.textContent = p.name;
      if (p.name === store.active_profile) opt.selected = true;
      select.appendChild(opt);
    });
    await loadConfig();
  } catch (err) {
    showMessage('settings-message', 'Failed to load profiles: ' + err, 'error');
  }
}

async function loadConfig() {
  try {
    const config = await invoke<Config>('get_config');
    (document.getElementById('server_address') as HTMLInputElement).value = config.server_address;
    (document.getElementById('ss_port') as HTMLInputElement).value = config.ss_port.toString();
    (document.getElementById('ss_password') as HTMLInputElement).value = config.ss_password;
    (document.getElementById('stls_port') as HTMLInputElement).value = config.stls_port.toString();
    (document.getElementById('stls_password') as HTMLInputElement).value = config.stls_password;
    (document.getElementById('stls_sni') as HTMLInputElement).value = config.stls_sni;
    (document.getElementById('socks5_port') as HTMLInputElement).value = config.socks5_port.toString();
    (document.getElementById('mtu') as HTMLInputElement).value = config.mtu ? config.mtu.toString() : '';
    (document.getElementById('split_rules') as HTMLTextAreaElement).value =
      config.split_rules.map(r => r.pattern).join('\n');
  } catch (err) {
    showMessage('settings-message', 'Failed to load config: ' + err, 'error');
  }
}

function getFormConfig(): Config {
  const mtuRaw = (document.getElementById('mtu') as HTMLInputElement).value;
  const splitRaw = (document.getElementById('split_rules') as HTMLTextAreaElement).value;
  const splitRules = splitRaw.split('\n').map(s => s.trim()).filter(s => s).map(s => ({ pattern: s }));

  return {
    server_address: (document.getElementById('server_address') as HTMLInputElement).value,
    ss_port: parseInt((document.getElementById('ss_port') as HTMLInputElement).value),
    ss_password: (document.getElementById('ss_password') as HTMLInputElement).value,
    stls_port: parseInt((document.getElementById('stls_port') as HTMLInputElement).value),
    stls_password: (document.getElementById('stls_password') as HTMLInputElement).value,
    stls_sni: (document.getElementById('stls_sni') as HTMLInputElement).value,
    socks5_port: parseInt((document.getElementById('socks5_port') as HTMLInputElement).value),
    mtu: mtuRaw ? parseInt(mtuRaw) : null,
    split_rules: splitRules,
  };
}

// ── Event listeners ────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
  // Tab switching
  document.querySelectorAll('.tab-btn').forEach(btn => {
    btn.addEventListener('click', () => switchTab(btn.dataset.tab!));
  });

  // Profile select
  document.getElementById('profile-select')?.addEventListener('change', async () => {
    const name = (document.getElementById('profile-select') as HTMLSelectElement).value;
    try {
      await invoke('switch_profile', { name });
      await loadConfig();
      showMessage('settings-message', 'Switched to ' + name, 'success');
    } catch (err) {
      showMessage('settings-message', 'Switch failed: ' + err, 'error');
    }
  });

  // New profile
  document.getElementById('btn-new-profile')?.addEventListener('click', async () => {
    const name = prompt('Enter profile name:');
    if (!name?.trim()) return;
    try {
      await invoke('add_profile', { name: name.trim(), config: getFormConfig() });
      await loadProfiles();
      showMessage('settings-message', 'Profile created!', 'success');
    } catch (err) {
      showMessage('settings-message', 'Failed to create: ' + err, 'error');
    }
  });

  // Delete profile
  document.getElementById('btn-delete-profile')?.addEventListener('click', async () => {
    const select = document.getElementById('profile-select') as HTMLSelectElement;
    const name = select.value;
    if (name === 'Default') { showMessage('settings-message', 'Cannot delete Default profile', 'error'); return; }
    if (!confirm(`Delete profile "${name}"?`)) return;
    try {
      await invoke('delete_profile', { name });
      await loadProfiles();
      showMessage('settings-message', 'Profile deleted', 'success');
    } catch (err) {
      showMessage('settings-message', 'Delete failed: ' + err, 'error');
    }
  });

  // Save profile config
  document.getElementById('profile-form')?.addEventListener('submit', async (e) => {
    e.preventDefault();
    try {
      await invoke('save_config', { config: getFormConfig() });
      showMessage('settings-message', 'Settings saved!', 'success');
    } catch (err) {
      showMessage('settings-message', 'Save failed: ' + err, 'error');
    }
  });

  // Save split rules
  document.getElementById('btn-save-split')?.addEventListener('click', async () => {
    try {
      const config = await invoke<Config>('get_config');
      const splitRaw = (document.getElementById('split_rules') as HTMLTextAreaElement).value;
      config.split_rules = splitRaw.split('\n').map(s => s.trim()).filter(s => s).map(s => ({ pattern: s }));
      await invoke('save_config', { config });
      showMessage('split-message', 'Split rules saved!', 'success');
    } catch (err) {
      showMessage('split-message', 'Save failed: ' + err, 'error');
    }
  });

  // Close window buttons
  document.getElementById('btn-back-app')?.addEventListener('click', () => window.close());
  document.getElementById('btn-back-profile')?.addEventListener('click', () => window.close());
  document.getElementById('btn-back-split')?.addEventListener('click', () => window.close());

  // Init
  loadProfiles();
});

function showMessage(id: string, text: string, type: 'success' | 'error') {
  const el = document.getElementById(id);
  if (!el) return;
  el.textContent = text;
  el.className = `message ${type}`;
  setTimeout(() => { el.textContent = ''; el.className = 'message'; }, 3000);
}
