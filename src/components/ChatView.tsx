// Chat header + message list + composer for the active character.

import { useEffect, useRef } from 'react';
import {
  ArrowLeft,
  Check,
  Cpu,
  EllipsisVertical,
  MessagesSquare,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  ScanText,
  Settings2,
  SquarePen,
  UserPen,
} from 'lucide-react';
import { activePersona, useApp } from '../lib/store';
import { characterAvatar, cn, IS_ANDROID, personaAvatar } from '../lib/utils';
import { Composer } from './Composer';
import { MessageItem } from './MessageItem';
import { Avatar, Button, IconButton, MenuItem, Popover } from './ui';

const PROVIDER_NAMES = { openai: 'OpenAI-compatible', claude: 'Claude', gemini: 'Gemini' } as const;

/** Who you are in this chat: tap to switch persona, or jump to the persona settings. */
function PersonaMenu() {
  const settings = useApp((s) => s.settings);
  const update = useApp((s) => s.updateSettings);
  const setSettingsTab = useApp((s) => s.setSettingsTab);
  const setPanel = useApp((s) => s.setPanel);
  if (!settings) return null;
  const current = activePersona(settings);
  const label = (p: { alias: string; name: string }) => p.alias || p.name || 'Unnamed';
  return (
    <Popover
      width="sm:w-64"
      trigger={({ toggle }) => (
        <button
          type="button"
          title={`Persona: ${label(current)}`}
          aria-label="Persona"
          onClick={toggle}
          className="grid h-9 w-9 shrink-0 place-items-center rounded-lg transition hover:bg-elevated"
        >
          <Avatar
            src={current.avatar ? personaAvatar(current.avatar) : null}
            name={current.name}
            className="h-7 w-7 rounded-full text-xs ring-1 ring-border"
          />
        </button>
      )}
    >
      {(close) => (
        <>
          <div className="px-3 pb-1 pt-1.5 text-[11px] font-medium uppercase tracking-wide text-muted">Chatting as</div>
          {settings.personas.map((p) => (
            <MenuItem
              key={p.id}
              className={cn(p.id === current.id && 'bg-border/40')}
              onClick={() => {
                update((s) => void (s.active_persona = p.id));
                close();
              }}
            >
              <Avatar
                src={p.avatar ? personaAvatar(p.avatar) : null}
                name={p.name}
                className="h-6 w-6 rounded-full text-[10px]"
              />
              <span className="min-w-0 flex-1 truncate">{label(p)}</span>
              {p.id === current.id && <Check size={14} className="shrink-0 text-accent" />}
            </MenuItem>
          ))}
          <div className="my-1 border-t border-border" />
          <MenuItem
            onClick={() => {
              setSettingsTab('persona');
              setPanel('settings');
              close();
            }}
          >
            <Settings2 size={16} className="text-accent" /> Manage personas…
          </MenuItem>
        </>
      )}
    </Popover>
  );
}

/** Everything not used every message: LLM connection, prompt inspector, character editor. */
function MoreMenu({ file }: { file: string }) {
  const provider = useApp((s) => s.settings?.provider ?? 'openai');
  const model = useApp((s) => s.settings?.connections[s.settings.provider].model ?? '');
  const setSettingsTab = useApp((s) => s.setSettingsTab);
  const setPanel = useApp((s) => s.setPanel);
  const openEditor = useApp((s) => s.openEditor);
  return (
    <Popover
      width="sm:w-64"
      trigger={({ toggle }) => (
        <IconButton label="More" onClick={toggle}>
          <EllipsisVertical size={18} />
        </IconButton>
      )}
    >
      {(close) => (
        <>
          <MenuItem
            onClick={() => {
              setSettingsTab('connection');
              setPanel('settings');
              close();
            }}
          >
            <Cpu size={16} className="shrink-0 text-accent" />
            <span className="min-w-0 flex-1 leading-tight">
              <span className="block">LLM settings</span>
              <span className="block truncate text-xs text-muted">
                {PROVIDER_NAMES[provider]} · {model || 'no model set'}
              </span>
            </span>
          </MenuItem>
          <MenuItem
            onClick={() => {
              setPanel('prompt');
              close();
            }}
          >
            <ScanText size={16} className="text-accent" /> Prompt inspector
          </MenuItem>
          <MenuItem
            onClick={() => {
              openEditor(file);
              close();
            }}
          >
            <UserPen size={16} className="text-accent" /> Edit character
          </MenuItem>
        </>
      )}
    </Popover>
  );
}

function EmptyState() {
  const openEditor = useApp((s) => s.openEditor);
  const hasCharacters = useApp((s) => s.characters.length > 0);
  return (
    <div className="flex h-full flex-col">
      <div className="grid flex-1 place-items-center px-6">
        <div className="animate-pop max-w-sm text-center">
          <div className="mx-auto mb-4 grid h-14 w-14 place-items-center rounded-2xl bg-elevated text-accent">
            <MessagesSquare size={26} />
          </div>
          <h1 className="text-lg font-semibold">{hasCharacters ? 'Pick a conversation' : 'Welcome'}</h1>
          <p className="mt-1 text-sm text-muted">
            {hasCharacters
              ? 'Choose someone from the list to continue a chat.'
              : 'Import a SillyTavern character card (PNG or JSON) or create a new character to begin.'}
          </p>
          <div className="mt-5 flex justify-center gap-2">
            <Button variant="primary" onClick={() => openEditor(null)}>
              <Plus size={15} /> New character
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

export function ChatView() {
  const card = useApp((s) => s.activeCard);
  const chat = useApp((s) => s.chat);
  const summary = useApp((s) => s.characters.find((c) => c.file === s.activeFile));
  const streamLength = useApp((s) => (s.streaming ? s.streaming.text.length + s.streaming.reasoning.length : 0));
  const setView = useApp((s) => s.setView);
  const sidebarHidden = useApp((s) => s.sidebarHidden);
  const toggleSidebar = useApp((s) => s.toggleSidebar);
  const typing = useApp((s) => s.streaming !== null && s.streaming.index !== -1);
  const setPanel = useApp((s) => s.setPanel);
  const newChat = useApp((s) => s.newChat);
  const scroller = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);
  /** Last scrollTop seen; our own scrolling only ever moves down, so moving up is always the user. */
  const lastTop = useRef(0);
  const touchY = useRef(0);

  useEffect(() => {
    stickToBottom.current = true;
  }, [chat?.file]);

  useEffect(() => {
    const el = scroller.current;
    if (!el || !stickToBottom.current) return;
    el.scrollTop = el.scrollHeight;
    lastTop.current = el.scrollTop;
  }, [chat?.file, chat?.messages.length, streamLength]);

  if (!card || !chat || !summary) return <EmptyState />;

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-2 border-b border-border bg-surface px-2 pb-2 pt-[calc(env(safe-area-inset-top)+0.5rem)] sm:px-3">
        <IconButton label="Back to messages" className="md:hidden" onClick={() => setView('home')}>
          <ArrowLeft size={20} />
        </IconButton>
        {!IS_ANDROID && (
          <IconButton
            label={sidebarHidden ? 'Show messages' : 'Hide messages'}
            className="hidden md:inline-flex"
            onClick={toggleSidebar}
          >
            {sidebarHidden ? <PanelLeftOpen size={18} /> : <PanelLeftClose size={18} />}
          </IconButton>
        )}
        <Avatar
          src={characterAvatar(summary.file, { thumb: true, version: summary.modified })}
          name={card.name}
          className="h-10 w-10 rounded-full ring-2 ring-accent/60 ring-offset-2 ring-offset-surface"
        />
        <div className="min-w-0 flex-1 pl-1 leading-tight">
          <div className="truncate text-[15px] font-semibold">{card.name}</div>
          {typing ? (
            <div className="animate-fade truncate text-xs font-medium text-accent">{card.name} is typing…</div>
          ) : (
            <div className="truncate text-xs text-muted">{chat.file.replace(/\.jsonl$/, '')}</div>
          )}
        </div>
        <PersonaMenu />
        <IconButton label="Chat history" onClick={() => setPanel('chats')}>
          <MessagesSquare size={18} />
        </IconButton>
        <IconButton label="New chat" onClick={() => void newChat()}>
          <SquarePen size={18} />
        </IconButton>
        <MoreMenu file={summary.file} />
      </header>

      <div
        ref={scroller}
        // Wheel and touch release the bottom at once, before a streamed token can snap the view back.
        onWheel={(e) => {
          if (e.deltaY < 0) stickToBottom.current = false;
        }}
        onTouchStart={(e) => {
          touchY.current = e.touches[0].clientY;
        }}
        onTouchMove={(e) => {
          if (e.touches[0].clientY > touchY.current) stickToBottom.current = false;
          touchY.current = e.touches[0].clientY;
        }}
        onScroll={(e) => {
          const el = e.currentTarget;
          const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 32;
          const up = el.scrollTop < lastTop.current - 1;
          // Shrinking content clamps scrollTop upward while still at the bottom; that is not the user.
          if (up && !atBottom) stickToBottom.current = false;
          else if (!up && atBottom) stickToBottom.current = true;
          lastTop.current = el.scrollTop;
        }}
        className="flex-1 overflow-y-auto"
      >
        <div key={chat.file} className="animate-fade mx-auto flex max-w-3xl flex-col px-3 py-4">
          {chat.messages.length === 0 && (
            <p className="py-10 text-center text-sm text-muted">No messages yet. Say hello.</p>
          )}
          {chat.messages.map((m, i) => (
            <MessageItem key={`${i}:${String(m.send_date ?? '')}`} message={m} index={i} isLast={i === chat.messages.length - 1} />
          ))}
        </div>
      </div>

      <Composer />
    </div>
  );
}
