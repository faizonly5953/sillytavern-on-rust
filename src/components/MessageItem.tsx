// One chat message: markdown, reasoning, edit/delete/copy, swipes.

import { memo, useEffect, useRef, useState } from 'react';
import { Check, ChevronLeft, ChevronRight, Copy, Pencil, Snail, Square, Trash2, X } from 'lucide-react';
import { api } from '../lib/api';
import { activePersona, useApp } from '../lib/store';
import type { ChatMessage } from '../lib/types';
import { characterAvatar, cn, errorMessage, newId, personaAvatar, stripHtmlComments } from '../lib/utils';
import { Markdown } from './Markdown';
import { Avatar, Button, IconButton, Textarea } from './ui';

interface Props {
  message: ChatMessage;
  index: number;
  isLast: boolean;
}

export const MessageItem = memo(function MessageItem({ message, index, isLast }: Props) {
  const streaming = useApp((s) => (s.streaming && s.streaming.index === index ? s.streaming : null));
  const busy = useApp((s) => s.streaming !== null);
  const settings = useApp((s) => s.settings);
  const summary = useApp((s) => s.characters.find((c) => c.file === s.activeFile));
  const editMessage = useApp((s) => s.editMessage);
  const deleteMessage = useApp((s) => s.deleteMessage);
  const swipe = useApp((s) => s.swipe);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [copied, setCopied] = useState(false);
  const setExplanation = useApp((s) => s.setExplanation);
  const toast = useApp((s) => s.toast);
  const [explaining, setExplaining] = useState<{ genId: string; text: string } | null>(null);
  const explainId = useRef<string | null>(null);

  // Leaving the chat (unmount) stops an explanation still being written.
  useEffect(() => () => void (explainId.current && api.stopGeneration(explainId.current)), []);

  const text = streaming ? streaming.base + streaming.text : message.mes;
  // What the reader sees: HTML comments are hidden notes (Edit and Copy still use the raw text).
  const hasVisibleText = stripHtmlComments(text).trim() !== '';
  const reasoning = streaming ? streaming.reasoning : (message.extra?.reasoning ?? '');
  const persona = settings ? activePersona(settings) : null;
  const avatar = message.is_user
    ? persona?.avatar
      ? personaAvatar(persona.avatar)
      : null
    : summary
      ? characterAvatar(summary.file, { thumb: true, version: summary.modified })
      : null;
  const swipes = message.swipes ?? [];
  const swipeId = message.swipe_id ?? 0;
  const showSwipes = !message.is_user && !busy && (swipes.length > 1 || isLast);
  const slothOn = !!settings?.ui.sloth_mode && !message.is_user && !message.is_system;
  const showSloth = slothOn && !streaming && !editing && hasVisibleText;
  const explanation = explaining ? explaining.text : (message.extra?.sloth ?? '');

  /** Sloth Mode: a separate generation that explains this message; it is stored beside it, never in it. */
  async function explain() {
    if (explaining) {
      void api.stopGeneration(explaining.genId);
      return;
    }
    const { chat, activeFile } = useApp.getState();
    if (!chat || !activeFile) return;
    const genId = newId();
    const mes = message.mes;
    let out = '';
    explainId.current = genId;
    setExplaining({ genId, text: '' });
    try {
      await api.generate(
        { gen_id: genId, char_file: activeFile, history: chat.messages.slice(0, index), mode: 'explain', input: mes },
        (event) => {
          if (event.type !== 'delta') return;
          out += event.text;
          setExplaining({ genId, text: out });
        },
      );
    } catch (e) {
      toast(errorMessage(e));
    }
    explainId.current = null;
    if (out.trim()) await setExplanation(chat.file, index, mes, out.trim());
    setExplaining(null);
  }

  function startEdit() {
    setDraft(message.mes);
    setEditing(true);
  }

  async function copy() {
    await navigator.clipboard.writeText(message.mes);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  }

  return (
    <div className={cn('group flex gap-3 rounded-2xl px-1 py-3', message.is_system && 'opacity-60')}>
      <Avatar src={avatar} name={message.name} className="mt-0.5 h-10 w-10 rounded-full" />
      <div className="min-w-0 flex-1">
        <div className="flex min-h-7 items-center gap-2">
          <span className="truncate text-sm font-semibold">{message.name}</span>
          {typeof message.send_date === 'string' && (
            <span className="hidden truncate text-[11px] text-muted sm:inline">{message.send_date}</span>
          )}
          {!editing && !streaming && (
            <div className="ml-auto flex items-center opacity-60 transition md:opacity-0 md:group-hover:opacity-100">
              <IconButton label="Copy" className="h-7 w-7" onClick={() => void copy()}>
                {copied ? <Check size={14} /> : <Copy size={14} />}
              </IconButton>
              <IconButton label="Edit" className="h-7 w-7" onClick={startEdit} disabled={busy}>
                <Pencil size={14} />
              </IconButton>
              <IconButton
                label={confirmDelete ? 'Click again to delete' : 'Delete'}
                className={cn('h-7 w-7', confirmDelete && 'bg-danger/15 text-danger hover:text-danger')}
                disabled={busy}
                onClick={() => {
                  if (confirmDelete) void deleteMessage(index);
                  else {
                    setConfirmDelete(true);
                    window.setTimeout(() => setConfirmDelete(false), 2500);
                  }
                }}
              >
                <Trash2 size={14} />
              </IconButton>
            </div>
          )}
        </div>

        {reasoning && (settings?.ui.show_reasoning ?? true) && (
          <details className="mb-2 rounded-lg border border-border bg-bg/60 px-3 py-2" open={!!streaming && !text}>
            <summary className="cursor-pointer select-none text-xs font-medium text-muted">Reasoning</summary>
            <div className="mt-2 max-h-64 overflow-y-auto whitespace-pre-wrap text-xs leading-relaxed text-muted">
              {reasoning}
            </div>
          </details>
        )}

        {editing ? (
          <div className="space-y-2">
            <Textarea autoFocus value={draft} onChange={(e) => setDraft(e.target.value)} className="min-h-32" />
            <div className="flex gap-2">
              <Button
                variant="primary"
                onClick={() => {
                  void editMessage(index, draft);
                  setEditing(false);
                }}
              >
                <Check size={15} /> Save
              </Button>
              <Button variant="ghost" onClick={() => setEditing(false)}>
                <X size={15} /> Cancel
              </Button>
            </div>
          </div>
        ) : streaming && !hasVisibleText ? (
          <div className="flex h-6 items-center gap-1" aria-label="Writing">
            {[0, 1, 2].map((i) => (
              <span key={i} className="typing-dot h-1.5 w-1.5 rounded-full bg-muted" style={{ animationDelay: `${i * 150}ms` }} />
            ))}
          </div>
        ) : !hasVisibleText && text ? (
          <p className="text-xs italic text-muted">Hidden note. Edit shows it.</p>
        ) : (
          <Markdown text={text} />
        )}

        {slothOn && !editing && (explaining || explanation) && (
          <details className="mt-2 rounded-lg border border-border bg-bg/60 px-3 py-2" open>
            <summary className="cursor-pointer select-none text-xs font-medium text-muted">Sloth Mode</summary>
            {explanation ? (
              <div className="mt-2 max-h-80 overflow-y-auto text-[13px] leading-relaxed text-muted">
                <Markdown text={explanation} />
              </div>
            ) : (
              <div className="mt-2 flex h-5 items-center gap-1" aria-label="Explaining">
                {[0, 1, 2].map((i) => (
                  <span key={i} className="typing-dot h-1.5 w-1.5 rounded-full bg-muted" style={{ animationDelay: `${i * 150}ms` }} />
                ))}
              </div>
            )}
          </details>
        )}

        {(showSwipes || showSloth) && (
          <div className="mt-1 flex items-center gap-0.5 text-xs text-muted">
            {showSwipes && (
              <>
                <IconButton label="Previous swipe" className="h-7 w-7" disabled={swipeId === 0} onClick={() => void swipe(index, -1)}>
                  <ChevronLeft size={16} />
                </IconButton>
                <span className="min-w-10 text-center tabular-nums">
                  {swipeId + 1}/{Math.max(swipes.length, 1)}
                </span>
                <IconButton
                  label={swipeId >= swipes.length - 1 ? 'Generate another reply' : 'Next swipe'}
                  className="h-7 w-7"
                  disabled={swipeId >= swipes.length - 1 && !isLast}
                  onClick={() => void swipe(index, 1)}
                >
                  <ChevronRight size={16} />
                </IconButton>
              </>
            )}
            {showSloth && (
              <IconButton
                label={explaining ? 'Stop explaining' : explanation ? 'Sloth Mode: explain again' : 'Sloth Mode: explain this message'}
                className={cn('h-7 w-7', explaining && 'text-accent')}
                disabled={busy && !explaining}
                onClick={() => void explain()}
              >
                {explaining ? <Square size={13} /> : <Snail size={16} />}
              </IconButton>
            )}
          </div>
        )}
      </div>
    </div>
  );
});
