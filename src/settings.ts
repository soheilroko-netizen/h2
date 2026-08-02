import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

interface Config {
  server_address: string;
  ss_port: number;
  ss_password: string;
  stls_port: number;
  stls_password: string;
  stls_sni: string;
  socks5_port: number;
  split_rules: Array<{ pattern: string }>;
  h2_port: number;
  h2_password: string;
  h2_sni: string;
  h2_insecure: boolean;
  h2_obfs: string;
  h2_obfs_password: string;
  h2_mport: string;
}

// Load config on init
window.addEventListener('DOMContentLoaded', async () => {
  await loadConfig();

  document.getElementById('btn-save-config')?.addEventListener('click', saveConfig);
  document.getElementById('btn-close-settings')?.addEventListener('click', () => {
    getCurrentWindow().close();
  });
});

async function loadConfig() {
  try {
    const config = await invoke<Config>('get_config');
    (document.getElementById('cfg-server-address') as HTMLInputElement).value = config.server_address;
    (document.getElementById('cfg-stls-port') as HTMLInputElement).value = String(config.stls_port);
    (document.getElementById('cfg-stls-password') as HTMLInputElement).value = config.stls_password;
    (document.getElementById('cfg-stls-sni') as HTMLInputElement).value = config.stls_sni;
    (document.getElementById('cfg-ss-port') as HTMLInputElement).value = String(config.ss_port);
    (document.getElementById('cfg-ss-password') as HTMLInputElement).value = config.ss_password;
    (document.getElementById('cfg-socks5-port') as HTMLInputElement).value = String(config.socks5_port);
    (document.getElementById('cfg-h2-port') as HTMLInputElement).value = String(config.h2_port);
    (document.getElementById('cfg-h2-password') as HTMLInputElement).value = config.h2_password;
    (document.getElementById('cfg-h2-sni') as HTMLInputElement).value = config.h2_sni;
    (document.getElementById('cfg-h2-insecure') as HTMLInputElement).checked = config.h2_insecure;
    (document.getElementById('cfg-h2-obfs') as HTMLInputElement).value = config.h2_obfs;
    (document.getElementById('cfg-h2-obfs-password') as HTMLInputElement).value = config.h2_obfs_password;
    (document.getElementById('cfg-h2-mport') as HTMLInputElement).value = config.h2_mport;
    (document.getElementById('cfg-split-domains') as HTMLTextAreaElement).value = 
      config.split_rules.map(r => r.pattern).join('\n');
  } catch (e) {
    showMessage('Failed to load config: ' + e, true);
  }
}

async function saveConfig() {
  const config = {
    server_address: (document.getElementById('cfg-server-address') as HTMLInputElement).value,
    stls_port: parseInt((document.getElementById('cfg-stls-port') as HTMLInputElement).value),
    stls_password: (document.getElementById('cfg-stls-password') as HTMLInputElement).value,
    stls_sni: (document.getElementById('cfg-stls-sni') as HTMLInputElement).value,
    ss_port: parseInt((document.getElementById('cfg-ss-port') as HTMLInputElement).value),
    ss_password: (document.getElementById('cfg-ss-password') as HTMLInputElement).value,
    socks5_port: parseInt((document.getElementById('cfg-socks5-port') as HTMLInputElement).value),
    h2_port: parseInt((document.getElementById('cfg-h2-port') as HTMLInputElement).value),
    h2_password: (document.getElementById('cfg-h2-password') as HTMLInputElement).value,
    h2_sni: (document.getElementById('cfg-h2-sni') as HTMLInputElement).value,
    h2_insecure: (document.getElementById('cfg-h2-insecure') as HTMLInputElement).checked,
    h2_obfs: (document.getElementById('cfg-h2-obfs') as HTMLInputElement).value,
    h2_obfs_password: (document.getElementById('cfg-h2-obfs-password') as HTMLInputElement).value,
    h2_mport: (document.getElementById('cfg-h2-mport') as HTMLInputElement).value,
    split_rules: (document.getElementById('cfg-split-domains') as HTMLTextAreaElement).value
      .split('\n')
      .map(s => s.trim())
      .filter(s => s.length > 0)
      .map(pattern => ({ pattern }))
  };

  try {
    await invoke('save_config', { config });
    showMessage('✓ Saved', false);
  } catch (e) {
    showMessage('Failed: ' + e, true);
  }
}

function showMessage(msg: string, isError: boolean) {
  const el = document.getElementById('settings-message');
  if (el) {
    el.textContent = msg;
    el.className = isError ? 'message error' : 'message success';
    setTimeout(() => { el.textContent = ''; el.className = 'message'; }, 3000);
  }
}
