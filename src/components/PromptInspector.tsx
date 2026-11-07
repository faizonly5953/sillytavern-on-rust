// Shows the exact prompt that will be sent, with token estimates.

import { useEffect, useState } from 'react';
import { api } from '../lib/api';
import { useApp } from '../lib/store';
import type { BuiltPrompt } from '../lib/types';
import { cn, errorMessage } from '../lib/utils';
import { Modal } from './ui';

const roleStyle = {
  system: 'bg-border/70 text-muted',
  user: 'bg-accent/15 text-accent',
  assistant: 'bg-quote/15 text-quote',
} as const;

export function PromptInspector() {
  const open = useApp((s) => s.panel === 'prompt');
  const setPanel = useApp((s) => s.setPanel);
  const activeFile = useApp((s) => s.activeFile);
  const chat = useApp((s) => s.chat);
  const flushSettings = useApp((s) => s.flushSettings);
  const [prompt, setPrompt] = useState<BuiltPrompt | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !activeFile || !chat) return;
    setPrompt(null);
    setError(null);
    flushSettings()
      .then(() => api.previewPrompt(activeFile, chat.messages, 'normal'))
      .then(setPrompt)
      .catch((e) => setError(errorMessage(e)));
  }, [open, activeFile, chat, flushSettings]);

  return (
    <Modal open={open} onClose={() => setPanel(null)} title="Prompt inspector" wide>
      {error && <p className="text-sm text-danger">{error}</p>}
      {!prompt && !error && <p className="text-sm text-muted">Building prompt…</p>}
      {prompt && (
        <div className="space-y-3">
          <div className="grid grid-cols-3 gap-2 text-center">
            {[
              [prompt.tokens_exact ? 'Tokens' : 'Tokens (≈)', `${prompt.total_tokens} / ${prompt.budget}`],
              ['History', `${prompt.history_included} / ${prompt.history_total}`],
              ['Messages', String(prompt.messages.length)],
            ].map(([label, value]) => (
              <div key={label} className="rounded-xl border border-border bg-bg px-2 py-2">
                <div className="text-[11px] uppercase tracking-wide text-muted">{label}</div>
                <div className="text-sm font-semibold tabular-nums">{value}</div>
              </div>
            ))}
          </div>
          {prompt.messages.map((m, i) => (
            <div key={i} className="rounded-xl border border-border bg-bg p-3">
              <div className="mb-1.5 flex items-center gap-2">
                <span className={cn('rounded-md px-1.5 py-0.5 text-[11px] font-semibold uppercase', roleStyle[m.role])}>{m.role}</span>
                <span className="text-[11px] text-muted">{prompt.tokens_exact ? '' : '~'}{m.tokens} tokens</span>
              </div>
              <pre className="whitespace-pre-wrap break-words font-sans text-xs leading-relaxed">{m.content}</pre>
            </div>
          ))}
          {prompt.prefill && (
            <div className="rounded-xl border border-dashed border-border p-3 text-xs">
              <span className="font-semibold">Prefill: </span>
              {prompt.prefill}
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}
