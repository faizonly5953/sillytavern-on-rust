// Character list: search, favorites, import (file, drag & drop, URL), create, sync status.

import { useMemo, useRef, useState, type DragEvent } from 'react';
import { Cloud, CloudOff, Link, Plus, RefreshCw, Search, Settings as SettingsIcon, Star, Upload } from 'lucide-react';
import { api } from '../lib/api';
import { activePersona, useApp } from '../lib/store';
import { characterAvatar, cn, errorMessage, formatDate, previewMacros } from '../lib/utils';
import { IconButton, Input } from './ui';

const CARD_FILE = /\.(png|json)$/i;

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

export function Sidebar() {
  const characters = useApp((s) => s.characters);
  const activeFile = useApp((s) => s.activeFile);
  const select = useApp((s) => s.selectCharacter);
  const openEditor = useApp((s) => s.openEditor);
  const setPanel = useApp((s) => s.setPanel);
  const refresh = useApp((s) => s.refreshCharacters);
  const importFiles = useApp((s) => s.importFiles);
  const importFromUrl = useApp((s) => s.importFromUrl);
  const setImportUrlOpen = useApp((s) => s.setImportUrlOpen);
  const toast = useApp((s) => s.toast);
  const userName = useApp((s) => (s.settings ? activePersona(s.settings).name : 'User'));
  const [query, setQuery] = useState('');
  const [dragging, setDragging] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return characters;
    return characters.filter(
      (c) => c.name.toLowerCase().includes(q) || c.tags.some((t) => t.toLowerCase().includes(q)),
    );
  }, [characters, query]);

  async function toggleFavorite(file: string, fav: boolean) {
    try {
      await api.setFavorite(file, fav);
      await refresh();
    } catch (e) {
      toast(errorMessage(e));
    }
  }

  function onDragOver(e: DragEvent) {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
    setDragging(true);
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    setDragging(false);
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
      onDragOver={onDragOver}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDragging(false);
      }}
      onDrop={onDrop}
      className="relative flex h-full flex-col border-r border-border bg-surface pt-[env(safe-area-inset-top)]"
    >
      <div className="flex items-center gap-1 px-4 pb-2 pt-3">
        <div className="flex-1 leading-tight">
          <div className="text-[15px] font-semibold tracking-tight">SillyTavern</div>
          <div className="text-xs text-muted">on Rust</div>
        </div>
        <SyncIndicator />
        <IconButton label="Settings" onClick={() => setPanel('settings')}>
          <SettingsIcon size={18} />
        </IconButton>
      </div>

      <div className="flex gap-1 px-3 pb-3">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted" size={15} />
          <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search" className="pl-8" />
        </div>
        <IconButton label="Import card file (PNG or JSON) — or drop files here" onClick={() => fileInput.current?.click()}>
          <Upload size={18} />
        </IconButton>
        <IconButton label="Import from URL" onClick={() => setImportUrlOpen(true)}>
          <Link size={17} />
        </IconButton>
        <IconButton label="New character" onClick={() => openEditor(null)}>
          <Plus size={18} />
        </IconButton>
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
      </div>

      <div className="flex-1 overflow-y-auto px-2 pb-3">
        {filtered.length === 0 && (
          <p className="px-4 py-10 text-center text-sm text-muted">
            {characters.length ? 'No matches.' : 'No characters yet. Import a card, drop one here, or create one.'}
          </p>
        )}
        {filtered.map((c) => (
          <div
            key={c.file}
            role="button"
            tabIndex={0}
            onClick={() => void select(c.file)}
            onKeyDown={(e) => e.key === 'Enter' && void select(c.file)}
            className={cn(
              'group flex cursor-pointer items-center gap-3 rounded-xl px-2 py-2 transition-colors',
              c.file === activeFile ? 'bg-elevated' : 'hover:bg-elevated/60',
            )}
          >
            <img
              src={characterAvatar(c.file, { thumb: true, version: c.modified })}
              alt=""
              loading="lazy"
              draggable={false}
              className="h-12 w-12 shrink-0 rounded-lg bg-elevated object-cover"
            />
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-medium">{c.name}</div>
              <div className="truncate text-xs text-muted">
                {previewMacros(c.description, c.name, userName) || c.tags.join(', ') || ' '}
              </div>
            </div>
            <button
              type="button"
              aria-label={c.fav ? 'Remove from favorites' : 'Add to favorites'}
              onClick={(e) => {
                e.stopPropagation();
                void toggleFavorite(c.file, !c.fav);
              }}
              className={cn(
                'rounded-md p-1.5 transition',
                c.fav ? 'text-quote' : 'text-muted opacity-40 md:opacity-0 md:group-hover:opacity-100',
              )}
            >
              <Star size={15} fill={c.fav ? 'currentColor' : 'none'} />
            </button>
          </div>
        ))}
      </div>

      {dragging && (
        <div className="pointer-events-none absolute inset-2 grid place-items-center rounded-2xl border-2 border-dashed border-accent bg-accent/10 backdrop-blur-[1px]">
          <div className="text-center">
            <Upload className="mx-auto mb-2 text-accent" size={26} />
            <div className="text-sm font-semibold">Drop to import</div>
            <div className="text-xs text-muted">PNG / JSON cards or a card link</div>
          </div>
        </div>
      )}
    </div>
  );
}
