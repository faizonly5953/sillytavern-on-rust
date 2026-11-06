// Chat header + message list + composer for the active character.

import { useEffect, useRef } from 'react';
import { ArrowLeft, MessagesSquare, Plus, ScanText, SquarePen, UserPen } from 'lucide-react';
import { useApp } from '../lib/store';
import { characterAvatar } from '../lib/utils';
import { Composer } from './Composer';
import { MessageItem } from './MessageItem';
import { Avatar, Button, IconButton } from './ui';

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
  const typing = useApp((s) => s.streaming !== null && s.streaming.index !== -1);
  const setPanel = useApp((s) => s.setPanel);
  const newChat = useApp((s) => s.newChat);
  const openEditor = useApp((s) => s.openEditor);
  const scroller = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  useEffect(() => {
    stickToBottom.current = true;
  }, [chat?.file]);

  useEffect(() => {
    const el = scroller.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [chat?.file, chat?.messages.length, streamLength]);

  if (!card || !chat || !summary) return <EmptyState />;

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-2 border-b border-border bg-surface px-2 pb-2 pt-[calc(env(safe-area-inset-top)+0.5rem)] sm:px-3">
        <IconButton label="Back to messages" className="md:hidden" onClick={() => setView('home')}>
          <ArrowLeft size={20} />
        </IconButton>
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
        <IconButton label="Chat history" onClick={() => setPanel('chats')}>
          <MessagesSquare size={18} />
        </IconButton>
        <IconButton label="New chat" onClick={() => void newChat()}>
          <SquarePen size={18} />
        </IconButton>
        <IconButton label="Prompt inspector" onClick={() => setPanel('prompt')}>
          <ScanText size={18} />
        </IconButton>
        <IconButton label="Edit character" onClick={() => openEditor(summary.file)}>
          <UserPen size={18} />
        </IconButton>
      </header>

      <div
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 96;
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
