// Character list: search, favorites, import, create.

import { useMemo, useRef, useState } from 'react';
import { Plus, Search, Settings as SettingsIcon, Star, Upload } from 'lucide-react';
import { api } from '../lib/api';
import { activePersona, useApp } from '../lib/store';
import { characterAvatar, cn, errorMessage, previewMacros, readFileBase64 } from '../lib/utils';
import { IconButton, Input } from './ui';

export function Sidebar() {
  const characters = useApp((s) => s.characters);
  const activeFile = useApp((s) => s.activeFile);
  const select = useApp((s) => s.selectCharacter);
  const openEditor = useApp((s) => s.openEditor);
  const setPanel = useApp((s) => s.setPanel);
  const refresh = useApp((s) => s.refreshCharacters);
  const toast = useApp((s) => s.toast);
  const userName = useApp((s) => (s.settings ? activePersona(s.settings).name : 'User'));
  const [query, setQuery] = useState('');
  const fileInput = useRef<HTMLInputElement>(null);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return characters;
    return characters.filter(
      (c) => c.name.toLowerCase().includes(q) || c.tags.some((t) => t.toLowerCase().includes(q)),
    );
  }, [characters, query]);

  async function importFiles(files: FileList | null) {
    if (!files?.length) return;
    let lastImported: string | null = null;
    let count = 0;
    for (const file of Array.from(files)) {
      try {
        lastImported = await api.importCharacter(await readFileBase64(file), file.name);
        count++;
      } catch (e) {
        toast(`${file.name}: ${errorMessage(e)}`);
      }
    }
    if (fileInput.current) fileInput.current.value = '';
    await refresh();
    if (lastImported) {
      toast(count > 1 ? `Imported ${count} characters` : 'Character imported', 'info');
      await select(lastImported);
    }
  }

  async function toggleFavorite(file: string, fav: boolean) {
    try {
      await api.setFavorite(file, fav);
      await refresh();
    } catch (e) {
      toast(errorMessage(e));
    }
  }

  return (
    <div className="flex h-full flex-col border-r border-border bg-surface pt-[env(safe-area-inset-top)]">
      <div className="flex items-center gap-2 px-4 pb-2 pt-3">
        <div className="flex-1 leading-tight">
          <div className="text-[15px] font-semibold tracking-tight">SillyTavern</div>
          <div className="text-xs text-muted">on Rust</div>
        </div>
        <IconButton label="Settings" onClick={() => setPanel('settings')}>
          <SettingsIcon size={18} />
        </IconButton>
      </div>

      <div className="flex gap-1.5 px-3 pb-3">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted" size={15} />
          <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search" className="pl-8" />
        </div>
        <IconButton label="Import card (PNG or JSON)" onClick={() => fileInput.current?.click()}>
          <Upload size={18} />
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
          onChange={(e) => void importFiles(e.target.files)}
        />
      </div>

      <div className="flex-1 overflow-y-auto px-2 pb-3">
        {filtered.length === 0 && (
          <p className="px-4 py-10 text-center text-sm text-muted">
            {characters.length ? 'No matches.' : 'No characters yet. Import a card or create one.'}
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
    </div>
  );
}
