// Import a character card from a website link (same sources as SillyTavern).

import { useEffect, useState } from 'react';
import { Link } from 'lucide-react';
import { useApp } from '../lib/store';
import { Button, Field, Input, Modal } from './ui';

const SOURCES = ['Chub', 'JanitorAI', 'Pygmalion', 'AICharacterCards', 'RisuRealm'];

export function ImportUrlDialog() {
  const open = useApp((s) => s.importUrlOpen);
  const setOpen = useApp((s) => s.setImportUrlOpen);
  const importFromUrl = useApp((s) => s.importFromUrl);
  const [url, setUrl] = useState('');
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) setUrl('');
  }, [open]);

  const submit = async () => {
    if (!url.trim() || busy) return;
    setBusy(true);
    const ok = await importFromUrl(url.trim());
    setBusy(false);
    if (ok) setOpen(false);
  };

  return (
    <Modal
      open={open}
      onClose={() => !busy && setOpen(false)}
      title="Import from URL"
      footer={
        <Button variant="primary" className="ml-auto" onClick={() => void submit()} disabled={!url.trim() || busy}>
          <Link size={15} /> {busy ? 'Downloading…' : 'Import'}
        </Button>
      }
    >
      <Field label="Character link" hint={`${SOURCES.join(', ')}, or a direct link to a PNG / JSON card.`}>
        <Input
          autoFocus
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && void submit()}
          placeholder="https://chub.ai/characters/creator/character-name"
          spellCheck={false}
          autoCapitalize="off"
        />
      </Field>
    </Modal>
  );
}
