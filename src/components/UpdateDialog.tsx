// "Update available" popup shown when a newer production release exists.
// Desktop: the updater installs in place and restarts. Android: the APK is downloaded here and
// handed to the system installer, after the one-time "Install unknown apps" permission.

import { useEffect, useRef, useState } from 'react';
import { Download, RefreshCw, Settings } from 'lucide-react';
import { useApp } from '../lib/store';
import {
  applyUpdate,
  canInstallInApp,
  downloadApk,
  installAllowed,
  installApk,
  openInstallSettings,
} from '../lib/updater';
import { errorMessage } from '../lib/utils';
import { Markdown } from './Markdown';
import { Button, Modal } from './ui';

/** Android in-app install: idle, waiting for the permission, downloading, installer opened. */
type Phase = 'idle' | 'permission' | 'downloading' | 'ready';

export function UpdateDialog() {
  const update = useApp((s) => s.update);
  const dismiss = useApp((s) => s.dismissUpdate);
  const toast = useApp((s) => s.toast);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [phase, setPhase] = useState<Phase>('idle');
  const apkPath = useRef<string | null>(null);
  const inApp = update !== null && canInstallInApp(update);

  const launchInstaller = () => {
    if (!apkPath.current) return;
    if (!installAllowed()) {
      setPhase('permission');
      openInstallSettings();
    } else if (installApk(apkPath.current)) {
      setPhase('ready');
    } else {
      toast('Could not open the installer. Try again.');
      apkPath.current = null;
      setPhase('idle');
    }
  };

  const startDownload = async () => {
    if (!update) return;
    setPhase('downloading');
    setProgress(null);
    try {
      apkPath.current = await downloadApk(update, setProgress);
      launchInstaller();
    } catch (e) {
      toast(`Update failed: ${errorMessage(e)}`);
      setPhase('idle');
    }
  };

  // Asks for the permission before downloading, so a refusal wastes no data.
  const begin = () => {
    if (apkPath.current) return launchInstaller();
    if (!installAllowed()) {
      setPhase('permission');
      openInstallSettings();
      return;
    }
    void startDownload();
  };

  // Back from the system settings screen: carry on by itself once the permission is on.
  useEffect(() => {
    if (phase !== 'permission') return;
    const back = () => {
      if (document.visibilityState !== 'visible' || !installAllowed()) return;
      stop();
      begin();
    };
    const stop = () => {
      document.removeEventListener('visibilitychange', back);
      window.removeEventListener('focus', back);
    };
    document.addEventListener('visibilitychange', back);
    window.addEventListener('focus', back);
    return stop;
  }, [phase]);

  if (!update) return null;

  const run = async () => {
    setBusy(true);
    setProgress(null);
    try {
      await applyUpdate(update, setProgress);
      if (!update.inPlace) dismiss();
    } catch (e) {
      toast(`Update failed: ${errorMessage(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const percent = progress !== null ? `${Math.round(progress * 100)}%` : null;
  const label = inApp
    ? {
        idle: 'Update now',
        permission: 'Open settings',
        downloading: percent ? `Downloading ${percent}` : 'Downloading…',
        ready: 'Install',
      }[phase]
    : busy
      ? percent
        ? `Downloading ${percent}`
        : 'Updating…'
      : update.inPlace
        ? 'Update & restart'
        : 'Download APK';
  const working = busy || phase === 'downloading';
  const Icon = phase === 'permission' ? Settings : update.inPlace ? RefreshCw : Download;

  return (
    <Modal
      open
      onClose={dismiss}
      required
      title="Update available"
      footer={
        <>
          <Button variant="ghost" onClick={dismiss} disabled={working}>
            Later
          </Button>
          <Button
            variant="primary"
            className="ml-auto"
            onClick={() => (inApp ? (phase === 'permission' ? openInstallSettings() : begin()) : void run())}
            disabled={working}
          >
            <Icon size={15} /> {label}
          </Button>
        </>
      }
    >
      <p className="text-sm">
        Version <span className="font-semibold">v{update.version}</span> is ready. You have v{update.current}.
      </p>
      <p className="mt-1 text-xs text-muted">
        {update.inPlace
          ? 'The app installs the update and restarts. Characters, chats and settings are kept.'
          : !inApp
            ? 'The new APK opens in your browser; tap it to install over this version. Your data is kept.'
            : phase === 'permission'
              ? 'Android needs your OK once: in the screen that just opened, allow “Install unknown apps” for this app, then come back. The update continues by itself.'
              : phase === 'ready'
                ? 'Confirm in the Android installer. If it did not appear, tap Install again.'
                : 'The app downloads the update and opens the Android installer; confirm there to install it over this version. Your data is kept.'}
      </p>
      {working && progress !== null && (
        <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-border">
          <div className="h-full bg-accent transition-all" style={{ width: `${progress * 100}%` }} />
        </div>
      )}
      {update.notes.trim() && (
        <div className="mt-4 rounded-xl border border-border bg-bg p-3">
          <div className="mb-1 text-xs font-medium uppercase tracking-wide text-muted">What's new</div>
          <Markdown text={update.notes} />
        </div>
      )}
    </Modal>
  );
}
