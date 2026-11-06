// "Create with AI": describe a character in plain words, the configured LLM writes the card,
// and it opens in the editor to be reviewed (and saved) like any new character.

import { useEffect, useRef, useState } from 'react';
import { Sparkles } from 'lucide-react';
import { api } from '../lib/api';
import { useApp } from '../lib/store';
import { emptyCharacter } from '../lib/types';
import { errorMessage, newId } from '../lib/utils';
import { Button, Field, Modal, Textarea } from './ui';

const EXAMPLE =
  'A sarcastic vampire hunter in a gothic city who secretly adores stray cats. Calm, dry humor, works alone. We meet while she is on a job.';

export function AiCharacterDialog() {
  const open = useApp((s) => s.aiCharacterOpen);
  const setOpen = useApp((s) => s.setAiCharacterOpen);
  const openEditor = useApp((s) => s.openEditor);
  const toast = useApp((s) => s.toast);
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const running = useRef<string | null>(null);

  useEffect(() => {
    if (open) setText('');
  }, [open]);

  const stop = () => {
    if (running.current) void api.stopGeneration(running.current);
  };

  const submit = async () => {
    if (!text.trim() || busy) return;
    const id = newId();
    running.current = id;
    setBusy(true);
    try {
      const card = await api.generateCharacter(id, text);
      if (!card) return; // stopped by the user
      setOpen(false);
      openEditor(null, { ...emptyCharacter(), ...card });
      toast('Review the character, then save it.', 'info');
    } catch (e) {
      toast(errorMessage(e));
    } finally {
      if (running.current === id) {
        running.current = null;
        setBusy(false);
      }
    }
  };

  return (
    <Modal
      open={open}
      onClose={() => {
        stop();
        setOpen(false);
      }}
      title="Create with AI"
      footer={
        <>
          <span className="min-w-0 flex-1 text-xs text-muted">Uses your LLM from Settings → Connection.</span>
          <Button variant="primary" onClick={() => void submit()} disabled={!text.trim() || busy}>
            <Sparkles size={15} className={busy ? 'animate-pulse' : undefined} /> {busy ? 'Writing…' : 'Create'}
          </Button>
        </>
      }
    >
      <Field
        label="Describe your character"
        hint="Who they are, their personality, the setting, how the chat should begin. More detail gives a closer result. {{user}} stands for you."
      >
        <Textarea
          autoFocus
          rows={7}
          value={text}
          disabled={busy}
          placeholder={EXAMPLE}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => (e.ctrlKey || e.metaKey) && e.key === 'Enter' && void submit()}
        />
      </Field>
    </Modal>
  );
}
