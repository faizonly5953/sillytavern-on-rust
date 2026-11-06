// "Update available" popup shown when a newer production release exists.

import { useState } from 'react';
import { Download, RefreshCw } from 'lucide-react';
import { useApp } from '../lib/store';
import { applyUpdate } from '../lib/updater';
import { errorMessage } from '../lib/utils';
import { Markdown } from './Markdown';
import { Button, Modal } from './ui';

export function UpdateDialog() {
  const update = useApp((s) => s.update);
  const dismiss = useApp((s) => s.dismissUpdate);
  const toast = useApp((s) => s.toast);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);

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

  const label = busy
    ? progress !== null
      ? `Downloading ${Math.round(progress * 100)}%`
      : 'Updating…'
    : update.inPlace
      ? 'Update & restart'
      : 'Download APK';

  return (
    <Modal
      open
      onClose={dismiss}
      required
      title="Update available"
      footer={
        <>
          <Button variant="ghost" onClick={dismiss} disabled={busy}>
            Later
          </Button>
          <Button variant="primary" className="ml-auto" onClick={() => void run()} disabled={busy}>
            {update.inPlace ? <RefreshCw size={15} /> : <Download size={15} />} {label}
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
          : 'The new APK opens in your browser; tap it to install over this version. Your data is kept.'}
      </p>
      {busy && progress !== null && (
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
