// Shown when items changed both on this device and on the server: pick a copy per item.

import { useEffect, useState } from 'react';
import { Cloud, HardDrive } from 'lucide-react';
import { useApp } from '../lib/store';
import type { SyncChoice, SyncConflict } from '../lib/types';
import { cn, formatDate } from '../lib/utils';
import { Button, Modal } from './ui';

function describe(conflict: SyncConflict, characterName: (file: string) => string) {
  switch (conflict.kind) {
    case 'character':
      return { title: characterName(conflict.id), subtitle: 'Character card' };
    case 'chat': {
      const [stem, file] = conflict.id.split('/');
      return { title: (file ?? conflict.id).replace(/\.jsonl$/, ''), subtitle: `Chat with ${characterName(`${stem}.png`)}` };
    }
    case 'settings':
      return { title: 'Settings', subtitle: 'Connection, generation, prompts, personas' };
    case 'secrets':
      return { title: 'API keys', subtitle: 'Encrypted with your passphrase' };
    default:
      return { title: conflict.id, subtitle: 'Persona avatar' };
  }
}

function side(exists: boolean, modified: number) {
  return exists ? formatDate(modified) || 'Changed' : 'Deleted';
}

export function ConflictDialog() {
  const conflicts = useApp((s) => s.conflicts);
  const syncing = useApp((s) => s.syncing);
  const characters = useApp((s) => s.characters);
  const syncNow = useApp((s) => s.syncNow);
  const dismiss = useApp((s) => s.dismissConflicts);
  const [choices, setChoices] = useState<Record<string, SyncChoice>>({});

  useEffect(() => setChoices({}), [conflicts]);

  if (!conflicts.length) return null;
  const characterName = (file: string) =>
    characters.find((c) => c.file === file)?.name ?? file.replace(/\.png$/, '');
  const chooseAll = (choice: SyncChoice) =>
    setChoices(Object.fromEntries(conflicts.map((c) => [c.key, choice])));
  const ready = conflicts.every((c) => choices[c.key]);

  return (
    <Modal
      open
      onClose={() => !syncing && dismiss()}
      title="Data differs between this device and the cloud"
      wide
      footer={
        <>
          <Button variant="ghost" onClick={dismiss} disabled={syncing}>
            Decide later
          </Button>
          <div className="ml-auto flex flex-wrap gap-2">
            <Button onClick={() => chooseAll('local')} disabled={syncing}>
              <HardDrive size={15} /> All local
            </Button>
            <Button onClick={() => chooseAll('cloud')} disabled={syncing}>
              <Cloud size={15} /> All cloud
            </Button>
            <Button
              variant="primary"
              disabled={!ready || syncing}
              onClick={() =>
                void syncNow({ manual: true, resolutions: conflicts.map((c) => ({ key: c.key, choice: choices[c.key] })) })
              }
            >
              {syncing ? 'Syncing…' : 'Apply & sync'}
            </Button>
          </div>
        </>
      }
    >
      <p className="mb-3 text-sm text-muted">
        These items were changed on this device and on another device since the last sync. Choose which copy to keep;
        the other one is replaced everywhere.
      </p>
      <div className="space-y-2">
        {conflicts.map((c) => {
          const info = describe(c, characterName);
          const pick = (choice: SyncChoice) => setChoices((prev) => ({ ...prev, [c.key]: choice }));
          return (
            <div key={c.key} className="rounded-xl border border-border bg-bg p-3">
              <div className="mb-2">
                <div className="truncate text-sm font-medium">{info.title}</div>
                <div className="truncate text-xs text-muted">{info.subtitle}</div>
              </div>
              <div className="grid grid-cols-2 gap-2">
                {(['local', 'cloud'] as const).map((choice) => (
                  <button
                    key={choice}
                    type="button"
                    onClick={() => pick(choice)}
                    className={cn(
                      'rounded-lg border px-3 py-2 text-left transition',
                      choices[c.key] === choice ? 'border-accent bg-accent/10' : 'border-border hover:bg-elevated',
                    )}
                  >
                    <div className="flex items-center gap-1.5 text-xs font-semibold">
                      {choice === 'local' ? <HardDrive size={13} /> : <Cloud size={13} />}
                      {choice === 'local' ? 'This device' : 'Cloud'}
                    </div>
                    <div className="mt-0.5 text-[11px] text-muted">
                      {choice === 'local' ? side(c.local_exists, c.local_modified) : side(c.cloud_exists, c.cloud_modified)}
                    </div>
                  </button>
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </Modal>
  );
}
