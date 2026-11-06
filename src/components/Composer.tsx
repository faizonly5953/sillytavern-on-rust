// Message input with send/stop, continue, impersonate, improve, regenerate.

import { useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react';
import { FastForward, PenLine, RefreshCw, SendHorizontal, Sparkles, Square, UserRound } from 'lucide-react';
import { useApp, type GenKind } from '../lib/store';
import { cn, isTouchDevice } from '../lib/utils';
import { Button, IconButton, inputClass } from './ui';

const actions: { kind: GenKind; label: string; icon: typeof RefreshCw }[] = [
  { kind: 'regenerate', label: 'Regenerate', icon: RefreshCw },
  { kind: 'continue', label: 'Continue', icon: FastForward },
  { kind: 'impersonate', label: 'Impersonate', icon: UserRound },
  { kind: 'improve', label: 'Improve my draft', icon: PenLine },
];

export function Composer() {
  const draft = useApp((s) => s.draft);
  const setDraft = useApp((s) => s.setDraft);
  const send = useApp((s) => s.send);
  const stop = useApp((s) => s.stop);
  const generate = useApp((s) => s.generate);
  const busy = useApp((s) => s.streaming !== null);
  const enterToSend = useApp((s) => s.settings?.ui.enter_to_send ?? true);
  const [menuOpen, setMenuOpen] = useState(false);
  const textarea = useRef<HTMLTextAreaElement>(null);

  useLayoutEffect(() => {
    const el = textarea.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
    el.style.overflowY = el.scrollHeight > 220 ? 'auto' : 'hidden';
  }, [draft]);

  function onKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key !== 'Enter' || e.shiftKey || e.nativeEvent.isComposing) return;
    if (!enterToSend || isTouchDevice()) return;
    e.preventDefault();
    if (!busy) void send(draft);
  }

  return (
    <div className="border-t border-border bg-surface px-3 pb-[calc(env(safe-area-inset-bottom)+0.75rem)] pt-3">
      <div className="mx-auto flex max-w-3xl items-end gap-2">
        <div className="relative">
          <IconButton label="More actions" active={menuOpen} disabled={busy} onClick={() => setMenuOpen((v) => !v)} className="h-10 w-10">
            <Sparkles size={18} />
          </IconButton>
          {menuOpen && (
            <>
              <div className="fixed inset-0 z-10" onClick={() => setMenuOpen(false)} />
              <div className="absolute bottom-12 left-0 z-20 w-48 rounded-xl border border-border bg-elevated p-1 shadow-xl">
                {actions.map(({ kind, label, icon: Icon }) => (
                  <button
                    key={kind}
                    type="button"
                    onClick={() => {
                      setMenuOpen(false);
                      void generate(kind);
                    }}
                    className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm hover:bg-border/60"
                  >
                    <Icon size={15} className="text-muted" /> {label}
                  </button>
                ))}
              </div>
            </>
          )}
        </div>
        <textarea
          ref={textarea}
          rows={1}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder={busy ? 'Writing…' : 'Message'}
          className={cn(inputClass,'max-h-[220px] min-h-10 flex-1 resize-none py-2.5 leading-relaxed')}
        />
        {busy ? (
          <Button variant="danger" onClick={() => void stop()} className="h-10 w-10 px-0" aria-label="Stop">
            <Square size={15} fill="currentColor" />
          </Button>
        ) : (
          <Button variant="primary" onClick={() => void send(draft)} className="h-10 w-10 px-0" aria-label="Send">
            <SendHorizontal size={17} />
          </Button>
        )}
      </div>
    </div>
  );
}
