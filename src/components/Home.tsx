// Home: search, "Recent" (story-style avatars) and "Messages" (conversations with
// last message and time). Also the import entry points: + menu, drag & drop, URL.

import { useMemo, useRef, useState, type DragEvent, type MouseEvent, type PointerEvent } from 'react';
import {
  Cloud,
  CloudOff,
  FileUp,
  Link,
  MessagesSquare,
  Plus,
  RefreshCw,
  Search,
  Settings as SettingsIcon,
  Star,
  Upload,
  UserPlus,
} from 'lucide-react';
import { api } from '../lib/api';
import { activePersona, useApp } from '../lib/store';
import type { CharacterSummary } from '../lib/types';
import { characterAvatar, cn, errorMessage, formatDate, plainPreview, previewMacros, timeAgo } from '../lib/utils';
import { Avatar, IconButton } from './ui';

const CARD_FILE = /\.(png|json)$/i;
const FRESH_MS = 60 * 60 * 1000;

// dragstart only fires for drags that begin inside the app (a link in a message,
// selected text); those must never count as an import drop.
let internalDrag = false;
window.addEventListener('dragstart', () => (internalDrag = true));
window.addEventListener('dragend', () => (internalDrag = false));

/** Lets a mouse drag scroll a horizontal strip; touch already scrolls natively. */
function useDragScroll() {
  const ref = useRef<HTMLDivElement>(null);
  const drag = useRef({ x: 0, left: 0, moved: false, down: false });
  return {
    ref,
    onPointerDown: (e: PointerEvent<HTMLDivElement>) => {
      if (e.pointerType !== 'mouse' || e.button !== 0) return;
      drag.current = { x: e.clientX, left: e.currentTarget.scrollLeft, moved: false, down: true };
    },
    onPointerMove: (e: PointerEvent<HTMLDivElement>) => {
      const d = drag.current;
      if (!d.down) return;
      const dx = e.clientX - d.x;
      if (!d.moved && Math.abs(dx) > 5) {
        d.moved = true;
        e.currentTarget.setPointerCapture(e.pointerId);
      }
      if (d.moved) e.currentTarget.scrollLeft = d.left - dx;
    },
    onPointerUp: () => void (drag.current.down = false),
    onPointerCancel: () => void (drag.current.down = false),
    // A drag ends with a click; it must not open the avatar under the cursor.
    onClickCapture: (e: MouseEvent) => {
      if (!drag.current.moved) return;
      drag.current.moved = false;
      e.preventDefault();
      e.stopPropagation();
    },
  };
}

function SyncIndicator() {
  const cloud = useApp((s) => s.cloud);
  const syncing = useApp((s) => s.syncing);
  const syncError = useApp((s) => s.syncError);
  const conflicts = useApp((s) => s.conflicts.length);
  const syncNow = useApp((s) => s.syncNow);
  if (!cloud?.enabled) return null;
  const label = syncing
    ? 'Syncing…'
    : conflicts
      ? `${conflicts} item(s) need a decision`
      : syncError
        ? `Sync problem: ${syncError}`
        : `Synced ${cloud.last_sync ? formatDate(cloud.last_sync) : ''} — click to sync now`;
  return (
    <IconButton
      label={label}
      onClick={() => void syncNow({ manual: true })}
      className={cn('relative', syncError && !syncing && 'text-danger', conflicts > 0 && 'text-quote')}
    >
      {syncing ? <RefreshCw size={17} className="animate-spin" /> : syncError ? <CloudOff size={18} /> : <Cloud size={18} />}
      {conflicts > 0 && !syncing && <span className="absolute right-1.5 top-1.5 h-2 w-2 rounded-full bg-quote" />}
    </IconButton>
  );
}

function NewMenu({ onImportFile }: { onImportFile: () => void }) {
  const [open, setOpen] = useState(false);
  const openEditor = useApp((s) => s.openEditor);
  const setImportUrlOpen = useApp((s) => s.setImportUrlOpen);
  const items = [
    { label: 'New character', icon: UserPlus, run: () => openEditor(null) },
    { label: 'Import card file', icon: FileUp, run: onImportFile },
    { label: 'Import from URL', icon: Link, run: () => setImportUrlOpen(true) },
  ];
  return (
    <div className="relative">
      <button
        type="button"
        aria-label="New or import"
        title="New or import"
        onClick={() => setOpen((v) => !v)}
        className={cn(
          'grid h-9 w-9 place-items-center rounded-xl bg-gradient-to-br from-accent to-quote text-accent-fg shadow-md shadow-accent/25 transition hover:brightness-110 active:scale-95',
          open && 'rotate-45',
        )}
      >
        <Plus size={18} />
      </button>
      {open && (
        <>
          <div className="fixed inset-0 z-20" onClick={() => setOpen(false)} />
          <div className="animate-pop absolute right-0 top-11 z-30 w-52 origin-top-right rounded-2xl border border-border bg-elevated p-1.5 shadow-xl">
            {items.map(({ label, icon: Icon, run }) => (
              <button
                key={label}
                type="button"
                onClick={() => {
                  setOpen(false);
                  run();
                }}
                className="flex w-full items-center gap-2.5 rounded-xl px-3 py-2.5 text-left text-sm transition hover:bg-border/60"
              >
                <Icon size={16} className="text-accent" /> {label}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function RecentAvatar({ character, active, index, onOpen }: { character: CharacterSummary; active: boolean; index: number; onOpen: () => void }) {
  return (
    <button
      type="button"
      onClick={onOpen}
      className="animate-rise group flex w-16 shrink-0 flex-col items-center gap-1.5"
      style={{ animationDelay: `${index * 30}ms` }}
    >
      <span
        className={cn(
          'rounded-full p-[2.5px] transition group-hover:scale-105 group-active:scale-95',
          active ? 'bg-accent' : 'bg-gradient-to-tr from-accent via-quote to-accent',
        )}
      >
        <Avatar
          src={characterAvatar(character.file, { thumb: true, version: character.modified })}
          name={character.name}
          className="h-14 w-14 rounded-full border-[2.5px] border-surface"
        />
      </span>
      <span className="w-full truncate text-center text-[11px] text-muted group-hover:text-fg">{character.name}</span>
    </button>
  );
}

function ConversationRow({
  character,
  active,
  index,
  userName,
  onOpen,
  onToggleFavorite,
}: {
  character: CharacterSummary;
  active: boolean;
  index: number;
  userName: string;
  onOpen: () => void;
  onToggleFavorite: () => void;
}) {
  const c = character;
  const fresh = c.last_active > 0 && Date.now() - c.last_active < FRESH_MS;
  const preview =
    plainPreview(previewMacros(c.last_message || c.description, c.name, userName)) || c.tags.join(', ') || ' ';
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => e.key === 'Enter' && onOpen()}
      className={cn(
        'animate-rise group flex cursor-pointer items-center gap-3 rounded-2xl px-2.5 py-2.5 transition-all duration-200 active:scale-[0.99]',
        active ? 'bg-elevated shadow-sm' : 'hover:bg-elevated/60',
      )}
      style={{ animationDelay: `${Math.min(index, 14) * 25}ms` }}
    >
      <div className="relative shrink-0">
        <Avatar
          src={characterAvatar(c.file, { thumb: true, version: c.modified })}
          name={c.name}
          className={cn(
            'h-12 w-12 rounded-full ring-2 ring-offset-2 ring-offset-surface transition',
            active ? 'ring-accent' : 'ring-border group-hover:ring-accent/50',
          )}
        />
        {c.fav && (
          <span className="absolute -bottom-0.5 -right-0.5 grid h-4 w-4 place-items-center rounded-full bg-surface text-quote">
            <Star size={10} fill="currentColor" />
          </span>
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="truncate text-sm font-semibold">{c.name}</span>
          <span className={cn('ml-auto shrink-0 text-[11px] tabular-nums', fresh ? 'font-semibold text-accent' : 'text-muted')}>
            {timeAgo(c.last_active)}
          </span>
        </div>
        <div className={cn('truncate text-xs', c.last_message ? 'text-muted' : 'italic text-muted/70')}>{preview}</div>
      </div>
      <button
        type="button"
        aria-label={c.fav ? 'Remove from favorites' : 'Add to favorites'}
        onClick={(e) => {
          e.stopPropagation();
          onToggleFavorite();
        }}
        className={cn(
          'shrink-0 rounded-lg p-1.5 transition',
          c.fav ? 'text-quote' : 'text-muted opacity-40 md:opacity-0 md:group-hover:opacity-100',
        )}
      >
        <Star size={15} fill={c.fav ? 'currentColor' : 'none'} />
      </button>
    </div>
  );
}

export function Home() {
  const characters = useApp((s) => s.characters);
  const activeFile = useApp((s) => s.activeFile);
  const select = useApp((s) => s.selectCharacter);
  const setView = useApp((s) => s.setView);
  const setPanel = useApp((s) => s.setPanel);
  const refresh = useApp((s) => s.refreshCharacters);
  const importFiles = useApp((s) => s.importFiles);
  const importFromUrl = useApp((s) => s.importFromUrl);
  const openEditor = useApp((s) => s.openEditor);
  const toast = useApp((s) => s.toast);
  const userName = useApp((s) => (s.settings ? activePersona(s.settings).name : 'User'));
  const [query, setQuery] = useState('');
  const [dragging, setDragging] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const recentStrip = useDragScroll();

  // Most recent conversation first; characters never chatted with follow, favorites first.
  const conversations = useMemo(
    () =>
      [...characters].sort(
        (a, b) => b.last_active - a.last_active || Number(b.fav) - Number(a.fav) || a.name.localeCompare(b.name),
      ),
    [characters],
  );
  const recent = conversations.filter((c) => c.last_active > 0).slice(0, 12);
  const q = query.trim().toLowerCase();
  const messages = q
    ? conversations.filter(
        (c) =>
          c.name.toLowerCase().includes(q) ||
          c.tags.some((t) => t.toLowerCase().includes(q)) ||
          c.last_message.toLowerCase().includes(q),
      )
    : conversations;

  const open = (file: string) => (file === activeFile ? setView('chat') : void select(file));

  async function toggleFavorite(file: string, fav: boolean) {
    try {
      await api.setFavorite(file, fav);
      await refresh();
    } catch (e) {
      toast(errorMessage(e));
    }
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    setDragging(false);
    if (internalDrag) return;
    const files = Array.from(e.dataTransfer.files);
    if (files.length) {
      const cards = files.filter((f) => CARD_FILE.test(f.name));
      if (cards.length) void importFiles(cards);
      else toast('Drop PNG or JSON character cards.');
      return;
    }
    // An image dragged straight from a browser arrives as a link.
    const text = e.dataTransfer.getData('text/uri-list') || e.dataTransfer.getData('text/plain');
    const url = text.split(/\r?\n/).map((l) => l.trim()).find((l) => /^https?:\/\//i.test(l));
    if (url) void importFromUrl(url);
  }

  return (
    <div
      onDragOver={(e) => {
        if (internalDrag) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = 'copy';
        setDragging(true);
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDragging(false);
      }}
      onDrop={onDrop}
      className="relative flex h-full flex-col overflow-hidden bg-surface pt-[env(safe-area-inset-top)]"
    >
      <div className="pointer-events-none absolute -top-24 left-1/2 h-56 w-[140%] -translate-x-1/2 rounded-full bg-gradient-to-b from-accent/20 via-quote/5 to-transparent blur-2xl" />

      <header className="relative flex items-center gap-2.5 px-5 pb-3 pt-4">
        <div className="grid h-10 w-10 place-items-center rounded-2xl bg-gradient-to-br from-accent to-quote text-accent-fg shadow-lg shadow-accent/25">
          <MessagesSquare size={19} />
        </div>
        <div className="min-w-0 flex-1 leading-tight">
          <div className="text-lg font-semibold tracking-tight">Messages</div>
          <div className="truncate text-xs text-muted">SillyTavern on Rust</div>
        </div>
        <SyncIndicator />
        <IconButton label="Settings" onClick={() => setPanel('settings')}>
          <SettingsIcon size={18} />
        </IconButton>
        <NewMenu onImportFile={() => fileInput.current?.click()} />
        <input
          ref={fileInput}
          type="file"
          accept=".png,.json,image/png,application/json"
          multiple
          hidden
          onChange={(e) => {
            const files = Array.from(e.target.files ?? []);
            e.target.value = '';
            if (files.length) void importFiles(files);
          }}
        />
      </header>

      <div className="relative px-5 pb-4">
        <div className="group flex items-center gap-2.5 rounded-2xl border border-transparent bg-elevated px-3.5 transition focus-within:border-accent/50 focus-within:bg-bg focus-within:shadow-[0_0_0_4px] focus-within:shadow-accent/10">
          <Search size={16} className="shrink-0 text-muted transition group-focus-within:text-accent" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search characters, tags, messages"
            className="h-11 w-full bg-transparent text-sm outline-none placeholder:text-muted/70"
          />
        </div>
      </div>

      <div className="relative flex-1 overflow-y-auto pb-[calc(env(safe-area-inset-bottom)+1.5rem)]">
        {recent.length > 0 && !q && (
          <section className="pb-3">
            <h2 className="px-5 pb-3 text-sm font-semibold">Recent</h2>
            <div {...recentStrip} className="scrollbar-none flex select-none gap-3 overflow-x-auto px-5 pb-1">
              {recent.map((c, i) => (
                <RecentAvatar key={c.file} character={c} active={c.file === activeFile} index={i} onOpen={() => open(c.file)} />
              ))}
            </div>
          </section>
        )}

        <section>
          <div className="flex items-baseline justify-between px-5 pb-2 pt-1">
            <h2 className="text-sm font-semibold">Messages</h2>
            <span className="text-xs text-muted">{messages.length}</span>
          </div>
          <div className="space-y-0.5 px-3">
            {messages.map((c, i) => (
              <ConversationRow
                key={c.file}
                character={c}
                active={c.file === activeFile}
                index={i}
                userName={userName}
                onOpen={() => open(c.file)}
                onToggleFavorite={() => void toggleFavorite(c.file, !c.fav)}
              />
            ))}
          </div>

          {messages.length === 0 && (
            <div className="animate-fade px-8 py-12 text-center">
              {characters.length ? (
                <p className="text-sm text-muted">Nothing matches “{query}”.</p>
              ) : (
                <>
                  <div className="mx-auto mb-3 grid h-14 w-14 place-items-center rounded-2xl bg-elevated text-accent">
                    <Upload size={24} />
                  </div>
                  <p className="text-sm font-medium">No characters yet</p>
                  <p className="mt-1 text-xs text-muted">Drop a PNG or JSON card here, or use + to create or import one.</p>
                  <button
                    type="button"
                    onClick={() => openEditor(null)}
                    className="mt-4 rounded-xl bg-accent px-4 py-2 text-sm font-medium text-accent-fg transition hover:brightness-110"
                  >
                    New character
                  </button>
                </>
              )}
            </div>
          )}
        </section>
      </div>

      {dragging && (
        <div className="animate-fade pointer-events-none absolute inset-2 grid place-items-center rounded-3xl border-2 border-dashed border-accent bg-accent/10 backdrop-blur-[2px]">
          <div className="text-center">
            <Upload className="mx-auto mb-2 text-accent" size={28} />
            <div className="text-sm font-semibold">Drop to import</div>
            <div className="text-xs text-muted">PNG / JSON cards or a card link</div>
          </div>
        </div>
      )}
    </div>
  );
}
