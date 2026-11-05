// Update flow. Desktop: tauri-plugin-updater downloads the signed installer listed in
// the release's latest.json, installs it and restarts. Android: Rust checks the latest
// GitHub release and we open its APK so the system installer takes over.

import { openUrl } from '@tauri-apps/plugin-opener';
import { relaunch } from '@tauri-apps/plugin-process';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { api } from './api';

export interface UpdateInfo {
  version: string;
  current: string;
  notes: string;
  /** true: installed in place (desktop). false: opens `downloadUrl` (Android). */
  inPlace: boolean;
  downloadUrl: string | null;
}

const isMobile = (os: string) => os === 'android' || os === 'ios';
let pending: Update | null = null;

export async function findUpdate(os: string): Promise<UpdateInfo | null> {
  if (isMobile(os)) {
    const release = await api.checkRelease();
    if (!release) return null;
    return {
      version: release.version,
      current: release.current,
      notes: release.notes,
      inPlace: false,
      downloadUrl: release.apk_url ?? release.page_url,
    };
  }
  pending = await check();
  if (!pending) return null;
  return {
    version: pending.version,
    current: pending.currentVersion,
    notes: pending.body ?? '',
    inPlace: true,
    downloadUrl: null,
  };
}

/** Desktop: download + install + restart (progress 0..1, or null when size is unknown). */
export async function applyUpdate(info: UpdateInfo, onProgress: (fraction: number | null) => void) {
  if (!info.inPlace || !pending) {
    if (info.downloadUrl) await openUrl(info.downloadUrl);
    return;
  }
  let total = 0;
  let received = 0;
  await pending.downloadAndInstall((event) => {
    if (event.event === 'Started') {
      total = event.data.contentLength ?? 0;
    } else if (event.event === 'Progress') {
      received += event.data.chunkLength;
      onProgress(total ? received / total : null);
    }
  });
  await relaunch();
}
