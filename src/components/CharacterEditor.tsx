// Create/edit a character card, avatar upload, delete.

import { useEffect, useRef, useState } from 'react';
import { ImagePlus, Plus, Save, Trash2, X } from 'lucide-react';
import { api } from '../lib/api';
import { useApp } from '../lib/store';
import { emptyCharacter, type CharacterData } from '../lib/types';
import { characterAvatar, errorMessage, readFileBase64 } from '../lib/utils';
import { Avatar, Button, Field, IconButton, Input, Modal, Textarea, Toggle } from './ui';

type TextKey =
  | 'name'
  | 'description'
  | 'personality'
  | 'scenario'
  | 'first_mes'
  | 'mes_example'
  | 'creator_notes'
  | 'system_prompt'
  | 'post_history_instructions'
  | 'creator'
  | 'character_version';

export function CharacterEditor() {
  const editor = useApp((s) => s.editor);
  const close = useApp((s) => s.closeEditor);
  const summary = useApp((s) => s.characters.find((c) => c.file === s.editor?.file));
  const characterSaved = useApp((s) => s.characterSaved);
  const characterDeleted = useApp((s) => s.characterDeleted);
  const toast = useApp((s) => s.toast);
  const [card, setCard] = useState<CharacterData>(emptyCharacter);
  const [avatar, setAvatar] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleteChats, setDeleteChats] = useState(true);
  const fileInput = useRef<HTMLInputElement>(null);
  const file = editor?.file ?? null;

  useEffect(() => {
    if (!editor) return;
    setAvatar(null);
    setConfirmDelete(false);
    if (!editor.file) {
      setCard(editor.draft ?? emptyCharacter());
      return;
    }
    api
      .getCharacter(editor.file)
      .then(setCard)
      .catch((e) => {
        toast(errorMessage(e));
        close();
      });
  }, [editor, close, toast]);

  if (!editor) return null;

  const set = (key: TextKey, value: string) => setCard((c) => ({ ...c, [key]: value }));
  const text = (key: TextKey, label: string, hint?: string, rows = 4) => (
    <Field label={label} hint={hint}>
      <Textarea rows={rows} value={String(card[key] ?? '')} onChange={(e) => set(key, e.target.value)} />
    </Field>
  );

  async function save() {
    if (!card.name.trim()) {
      toast('Name is required');
      return;
    }
    setSaving(true);
    try {
      let target = file;
      if (target) await api.updateCharacter(target, card, avatar);
      else target = await api.createCharacter(card, avatar);
      close();
      await characterSaved(target);
    } catch (e) {
      toast(errorMessage(e));
    } finally {
      setSaving(false);
    }
  }

  async function remove() {
    if (!file) return;
    try {
      await api.deleteCharacter(file, deleteChats);
      close();
      await characterDeleted(file);
    } catch (e) {
      toast(errorMessage(e));
    }
  }

  const preview = avatar ?? (summary ? characterAvatar(summary.file, { version: summary.modified }) : null);

  return (
    <Modal
      open
      onClose={close}
      wide
      title={file ? `Edit ${card.name || 'character'}` : 'New character'}
      footer={
        <>
          {file &&
            (confirmDelete ? (
              <div className="flex flex-wrap items-center gap-3">
                <Button variant="danger" onClick={() => void remove()}>
                  <Trash2 size={15} /> Confirm delete
                </Button>
                <div className="w-44">
                  <Toggle label="Also delete chats" checked={deleteChats} onChange={setDeleteChats} />
                </div>
              </div>
            ) : (
              <Button variant="ghost" onClick={() => setConfirmDelete(true)}>
                <Trash2 size={15} /> Delete
              </Button>
            ))}
          <div className="ml-auto flex gap-2">
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button variant="primary" onClick={() => void save()} disabled={saving}>
              <Save size={15} /> {saving ? 'Saving…' : 'Save'}
            </Button>
          </div>
        </>
      }
    >
      <div className="space-y-5">
        <div className="flex items-start gap-4">
          <button type="button" onClick={() => fileInput.current?.click()} className="group relative shrink-0" aria-label="Change avatar">
            <Avatar src={preview} name={card.name} className="h-28 w-20 rounded-xl text-2xl sm:h-36 sm:w-24" />
            <span className="absolute inset-0 grid place-items-center rounded-xl bg-black/50 text-white opacity-0 transition group-hover:opacity-100">
              <ImagePlus size={20} />
            </span>
          </button>
          <input
            ref={fileInput}
            type="file"
            accept="image/*"
            hidden
            onChange={async (e) => {
              const picked = e.target.files?.[0];
              if (picked) setAvatar(await readFileBase64(picked));
              e.target.value = '';
            }}
          />
          <div className="min-w-0 flex-1 space-y-3">
            <Field label="Name">
              <Input value={card.name} onChange={(e) => set('name', e.target.value)} placeholder="Character name" />
            </Field>
            <Field label="Tags" hint="Comma separated.">
              <Input
                value={card.tags.join(', ')}
                onChange={(e) =>
                  setCard((c) => ({ ...c, tags: e.target.value.split(',').map((t) => t.trim()).filter(Boolean) }))
                }
              />
            </Field>
          </div>
        </div>

        {text('description', 'Description', 'Who the character is. Sent with every message.', 6)}
        {text('first_mes', 'First message', 'The greeting that opens a new chat.', 5)}

        <div className="space-y-2">
          <span className="text-xs font-medium uppercase tracking-wide text-muted">Alternate greetings</span>
          {card.alternate_greetings.map((greeting, i) => (
            <div key={i} className="flex gap-2">
              <Textarea
                rows={3}
                value={greeting}
                onChange={(e) =>
                  setCard((c) => ({
                    ...c,
                    alternate_greetings: c.alternate_greetings.map((g, j) => (j === i ? e.target.value : g)),
                  }))
                }
              />
              <IconButton
                label="Remove greeting"
                onClick={() => setCard((c) => ({ ...c, alternate_greetings: c.alternate_greetings.filter((_, j) => j !== i) }))}
              >
                <X size={16} />
              </IconButton>
            </div>
          ))}
          <Button variant="ghost" onClick={() => setCard((c) => ({ ...c, alternate_greetings: [...c.alternate_greetings, ''] }))}>
            <Plus size={15} /> Add greeting
          </Button>
        </div>

        {text('personality', 'Personality', undefined, 3)}
        {text('scenario', 'Scenario', undefined, 3)}
        {text('mes_example', 'Example dialogue', 'Separate examples with <START>.', 5)}

        <details className="rounded-xl border border-border px-3 py-2">
          <summary className="cursor-pointer select-none py-1 text-sm font-medium">Advanced</summary>
          <div className="space-y-5 py-3">
            {text('system_prompt', 'System prompt override', 'Replaces the main prompt. Use {{original}} to include it.', 3)}
            {text('post_history_instructions', 'Post-history instructions', 'Use {{original}} to include the global one.', 3)}
            {text('creator_notes', 'Creator notes', 'Not sent to the model.', 3)}
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label="Creator">
                <Input value={card.creator} onChange={(e) => set('creator', e.target.value)} />
              </Field>
              <Field label="Version">
                <Input value={card.character_version} onChange={(e) => set('character_version', e.target.value)} />
              </Field>
            </div>
          </div>
        </details>
      </div>
    </Modal>
  );
}
