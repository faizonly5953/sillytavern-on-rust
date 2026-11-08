// Chat history list for the active character: open, new, rename, delete.

import { useEffect, useState } from 'react';
import { Check, Download, Pencil, Plus, Trash2, X } from 'lucide-react';
import { api } from '../lib/api';
import { useApp } from '../lib/store';
import type { ChatSummary } from '../lib/types';
import { cn, errorMessage, formatDate } from '../lib/utils';
import { Button, IconButton, Input, Modal } from './ui';

export function ChatsPanel() {
  const open = useApp((s) => s.panel === 'chats');
  const setPanel = useApp((s) => s.setPanel);
  const activeFile = useApp((s) => s.activeFile);
  const currentChat = useApp((s) => s.chat?.file);
  const openChat = useApp((s) => s.openChat);
  const newChat = useApp((s) => s.newChat);
  const toast = useApp((s) => s.toast);
  const [chats, setChats] = useState<ChatSummary[]>([]);
  const [renaming, setRenaming] = useState<{ file: string; name: string } | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);

  async function reload() {
    if (!activeFile) return;
    try {
      setChats(await api.listChats(activeFile));
    } catch (e) {
      toast(errorMessage(e));
    }
  }

  useEffect(() => {
    if (open) void reload();
    setRenaming(null);
    setConfirming(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, activeFile]);

  async function rename() {
    if (!activeFile || !renaming) return;
    try {
      const file = await api.renameChat(activeFile, renaming.file, renaming.name);
      if (renaming.file === currentChat) await openChat(file);
      setRenaming(null);
      await reload();
    } catch (e) {
      toast(errorMessage(e));
    }
  }

  async function exportChat(file: string) {
    if (!activeFile) return;
    try {
      const saved = await api.exportChat(activeFile, file);
      if (saved) toast(`Saved ${saved}`, 'info');
    } catch (e) {
      toast(errorMessage(e));
    }
  }

  async function remove(file: string) {
    if (!activeFile) return;
    try {
      await api.deleteChat(activeFile, file);
      if (file === currentChat) {
        const rest = chats.filter((c) => c.file !== file);
        if (rest[0]) await openChat(rest[0].file);
        else await newChat();
        setPanel('chats');
      }
      setConfirming(null);
      await reload();
    } catch (e) {
      toast(errorMessage(e));
    }
  }

  return (
    <Modal
      open={open}
      onClose={() => setPanel(null)}
      title="Chat history"
      footer={
        <Button variant="primary" onClick={() => void newChat()}>
          <Plus size={15} /> New chat
        </Button>
      }
    >
      <div className="space-y-1.5">
        {chats.map((c) => (
          <div
            key={c.file}
            className={cn(
              'rounded-xl border px-3 py-2.5 transition',
              c.file === currentChat ? 'border-accent/50 bg-accent/5' : 'border-border hover:bg-elevated',
            )}
          >
            {renaming?.file === c.file ? (
              <div className="flex gap-2">
                <Input
                  autoFocus
                  value={renaming.name}
                  onChange={(e) => setRenaming({ file: c.file, name: e.target.value })}
                  onKeyDown={(e) => e.key === 'Enter' && void rename()}
                />
                <IconButton label="Save name" onClick={() => void rename()}>
                  <Check size={16} />
                </IconButton>
                <IconButton label="Cancel" onClick={() => setRenaming(null)}>
                  <X size={16} />
                </IconButton>
              </div>
            ) : (
              <div className="flex items-start gap-2">
                <button type="button" className="min-w-0 flex-1 text-left" onClick={() => void openChat(c.file)}>
                  <div className="truncate text-sm font-medium">{c.file.replace(/\.jsonl$/, '')}</div>
                  <div className="truncate text-xs text-muted">{c.last_message || 'Empty chat'}</div>
                  <div className="mt-0.5 text-[11px] text-muted">
                    {c.message_count} messages · {formatDate(c.modified)}
                  </div>
                </button>
                <IconButton label="Export as JSONL" className="h-8 w-8" onClick={() => void exportChat(c.file)}>
                  <Download size={14} />
                </IconButton>
                <IconButton label="Rename" className="h-8 w-8" onClick={() => setRenaming({ file: c.file, name: c.file.replace(/\.jsonl$/, '') })}>
                  <Pencil size={14} />
                </IconButton>
                <IconButton
                  label={confirming === c.file ? 'Click again to delete' : 'Delete'}
                  className={cn('h-8 w-8', confirming === c.file && 'bg-danger/15 text-danger hover:text-danger')}
                  onClick={() => (confirming === c.file ? void remove(c.file) : setConfirming(c.file))}
                >
                  <Trash2 size={14} />
                </IconButton>
              </div>
            )}
          </div>
        ))}
        {chats.length === 0 && <p className="py-6 text-center text-sm text-muted">No chats yet.</p>}
      </div>
    </Modal>
  );
}
