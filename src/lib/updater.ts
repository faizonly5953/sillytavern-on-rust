// Update flow. Desktop: tauri-plugin-updater downloads the signed installer listed in
// the release's latest.json, installs it and restarts. Android: Rust checks the latest
// GitHub release, downloads its APK into the app cache, and the system installer takes over
// through the `SillyInstaller` bridge from MainActivity (no browser or GitHub visit). Without
// that bridge or an APK asset, the release link is opened instead.

import { openUrl } from '@tauri-apps/plugin-opener';
import { relaunch } from '@tauri-apps/plugin-process';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { api } from './api';

export interface UpdateInfo {
  version: string;
  current: string;
  notes: string;
  /** true: installed in place (desktop). false: Android. */
  inPlace: boolean;
  /** Android: the release's APK. */
  apkUrl: string | null;
  /** Android fallback when the release has no APK: its page. */
  pageUrl: string | null;
}

/** Added to the WebView by MainActivity (ApkInstaller.kt); absent everywhere else. */
interface InstallerBridge {
  canInstall(): boolean;
  openInstallSettings(): void;
  install(path: string): boolean;
}
const bridge = () => (window as unknown as { SillyInstaller?: InstallerBridge }).SillyInstaller;

/** True when the APK can be downloaded and installed from inside the app. */
export const canInstallInApp = (info: UpdateInfo) => !info.inPlace && info.apkUrl !== null && bridge() !== undefined;
/** Android 8+: has the user allowed this app to install apps ("Install unknown apps")? */
export const installAllowed = () => bridge()?.canInstall() ?? false;
export const openInstallSettings = () => bridge()?.openInstallSettings();
/** Downloads the APK (progress 0..1, or null when the size is unknown); resolves with its path. */
export const downloadApk = (info: UpdateInfo, onProgress: (fraction: number | null) => void) =>
  api.downloadUpdate(info.apkUrl ?? '', (p) => onProgress(p.total ? p.received / p.total : null));
/** Opens Android's installer for a downloaded APK. */
export const installApk = (path: string) => bridge()?.install(path) ?? false;

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
      apkUrl: release.apk_url,
      pageUrl: release.page_url,
    };
  }
  pending = await check();
  if (!pending) return null;
  return {
    version: pending.version,
    current: pending.currentVersion,
    notes: pending.body ?? '',
    inPlace: true,
    apkUrl: null,
    pageUrl: null,
  };
}

/** Desktop: download + install + restart (progress 0..1, or null when size is unknown). Android without the bridge: opens the download link. */
export async function applyUpdate(info: UpdateInfo, onProgress: (fraction: number | null) => void) {
  if (!info.inPlace || !pending) {
    const url = info.apkUrl ?? info.pageUrl;
    if (url) await openUrl(url);
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
