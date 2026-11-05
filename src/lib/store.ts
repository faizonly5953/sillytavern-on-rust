// Zustand store: settings, characters, active chat, generation/streaming state.

import { create } from 'zustand';
import { api } from './api';
import { findUpdate, type UpdateInfo } from './updater';
import type {
  AppInfo,
  CharacterData,
  CharacterSummary,
  ChatFile,
  ChatMessage,
  GenMode,
  Persona,
  Provider,
  Settings,
  SwipeInfo,
} from './types';
import { errorMessage, newId, sendDateNow } from './utils';

export type Panel = 'settings' | 'chats' | 'prompt' | null;
export type GenKind = 'normal' | 'regenerate' | 'continue' | 'impersonate';

interface Toast {
  id: string;
  kind: 'error' | 'info';
  text: string;
}

/** The message being written. `index` is -1 for impersonation (goes to the composer). */
export interface Streaming {
  genId: string;
  kind: GenKind;
  index: number;
  base: string;
  text: string;
  reasoning: string;
}

interface AppStore {
  ready: boolean;
  settings: Settings | null;
  secrets: Record<Provider, string | null>;
  characters: CharacterSummary[];
  activeFile: string | null;
  activeCard: CharacterData | null;
  chat: ChatFile | null;
  streaming: Streaming | null;
  draft: string;
  panel: Panel;
  editor: { file: string | null } | null;
  sidebarOpen: boolean;
  toasts: Toast[];
  appInfo: AppInfo | null;
  update: UpdateInfo | null;
  checkingUpdate: boolean;

  init(): Promise<void>;
  checkForUpdates(manual: boolean): Promise<void>;
  dismissUpdate(): void;
  toast(text: string, kind?: Toast['kind']): void;
  dismissToast(id: string): void;
  setPanel(panel: Panel): void;
  setSidebar(open: boolean): void;
  openEditor(file: string | null): void;
  closeEditor(): void;
  setDraft(text: string): void;
  updateSettings(mutate: (draft: Settings) => void): void;
  replaceSettings(settings: Settings): void;
  flushSettings(): Promise<void>;
  refreshSecrets(): Promise<void>;
  refreshCharacters(): Promise<void>;
  selectCharacter(file: string, chatFile?: string | null): Promise<void>;
  openChat(chatFile: string): Promise<void>;
  newChat(): Promise<void>;
  characterSaved(file: string): Promise<void>;
  characterDeleted(file: string): Promise<void>;
  send(text: string): Promise<void>;
  generate(kind: GenKind): Promise<void>;
  stop(): Promise<void>;
  editMessage(index: number, text: string): Promise<void>;
  deleteMessage(index: number): Promise<void>;
  swipe(index: number, direction: -1 | 1): Promise<void>;
}

export function activePersona(settings: Settings): Persona {
  return (
    settings.personas.find((p) => p.id === settings.active_persona) ??
    settings.personas[0] ?? { id: 'default', name: 'User', description: '', avatar: null }
  );
}

let saveTimer: number | undefined;
let pendingSave: Settings | null = null;

function swipeReasoning(message: ChatMessage, id: number): string | undefined {
  return message.swipe_info?.[id]?.extra?.reasoning;
}

/** Writes text (and reasoning) into the message and its current swipe. */
function withText(message: ChatMessage, text: string, reasoning?: string): ChatMessage {
  const id = message.swipe_id ?? 0;
  const swipes = message.swipes ? [...message.swipes] : undefined;
  if (swipes) swipes[id] = text;
  const extra = { ...(message.extra ?? {}) };
  let swipe_info = message.swipe_info;
  if (reasoning !== undefined) {
    if (reasoning) extra.reasoning = reasoning;
    else delete extra.reasoning;
    if (swipes) {
      swipe_info = [...(swipe_info ?? [])];
      while (swipe_info.length < swipes.length) swipe_info.push({});
      const info: SwipeInfo = { ...swipe_info[id] };
      info.extra = { ...(info.extra ?? {}), reasoning: reasoning || undefined };
      swipe_info[id] = info;
    }
  }
  return { ...message, mes: text, swipes, swipe_info, extra };
}

export const useApp = create<AppStore>((set, get) => {
  const persist = async () => {
    const { activeFile, chat } = get();
    if (!activeFile || !chat) return;
    try {
      await api.saveChat(activeFile, chat);
    } catch (e) {
      get().toast(`Could not save chat: ${errorMessage(e)}`);
    }
  };

  const setMessages = (messages: ChatMessage[]) => {
    const chat = get().chat;
    if (chat) set({ chat: { ...chat, messages } });
  };

  const finishGeneration = async (genId: string, failure: string | null) => {
    const s = get().streaming;
    const chat = get().chat;
    if (!s || s.genId !== genId) return;
    if (failure) get().toast(failure);

    if (s.index === -1) {
      const draft = get().draft.trim();
      set({ streaming: null, draft: draft ? `${draft} ${s.text.trim()}` : s.text.trim() });
      return;
    }
    if (!chat) {
      set({ streaming: null });
      return;
    }
    const messages = [...chat.messages];
    const target = messages[s.index];
    const empty = !s.text.trim();
    if (target && empty && s.kind === 'normal') {
      messages.splice(s.index, 1);
    } else if (target && empty && s.kind === 'regenerate') {
      const swipes = (target.swipes ?? []).slice(0, -1);
      const id = Math.max(0, swipes.length - 1);
      messages[s.index] = {
        ...target,
        swipes,
        swipe_id: id,
        swipe_info: target.swipe_info?.slice(0, swipes.length),
        mes: swipes[id] ?? '',
        extra: { ...(target.extra ?? {}), reasoning: swipeReasoning(target, id) },
      };
    } else if (target && !empty) {
      const reasoning = s.kind === 'continue' ? (target.extra?.reasoning ?? '') + s.reasoning : s.reasoning;
      messages[s.index] = withText(target, s.base + s.text, reasoning);
    }
    set({ chat: { ...chat, messages }, streaming: null });
    await persist();
  };

  return {
    ready: false,
    settings: null,
    secrets: { openai: null, claude: null, gemini: null },
    characters: [],
    activeFile: null,
    activeCard: null,
    chat: null,
    streaming: null,
    draft: '',
    panel: null,
    editor: null,
    sidebarOpen: false,
    toasts: [],
    appInfo: null,
    update: null,
    checkingUpdate: false,

    async init() {
      try {
        const [settings, secrets, characters, appInfo] = await Promise.all([
          api.getSettings(),
          api.getSecretStatus(),
          api.listCharacters(),
          api.appInfo(),
        ]);
        set({ settings, secrets, characters, appInfo, ready: true });
        // Quiet check shortly after start; failures (offline, no release yet) stay silent.
        window.setTimeout(() => void get().checkForUpdates(false), 2500);
        const last = settings.ui.last_character;
        if (last && characters.some((c) => c.file === last)) {
          await get().selectCharacter(last, settings.ui.last_chat);
        } else if (!characters.length) {
          set({ sidebarOpen: true });
        }
      } catch (e) {
        set({ ready: true });
        get().toast(errorMessage(e));
      }
    },

    async checkForUpdates(manual) {
      const info = get().appInfo;
      if (!info || get().checkingUpdate) return;
      set({ checkingUpdate: true });
      try {
        const update = await findUpdate(info.os);
        if (update) set({ update });
        else if (manual) get().toast(`You're on the latest version (v${info.version}).`, 'info');
      } catch (e) {
        if (manual) get().toast(`Update check failed: ${errorMessage(e)}`);
      } finally {
        set({ checkingUpdate: false });
      }
    },
    dismissUpdate: () => set({ update: null }),

    toast(text, kind = 'error') {
      const id = newId();
      set({ toasts: [...get().toasts, { id, kind, text }] });
      window.setTimeout(() => get().dismissToast(id), kind === 'error' ? 7000 : 3500);
    },
    dismissToast(id) {
      set({ toasts: get().toasts.filter((t) => t.id !== id) });
    },
    setPanel: (panel) => set({ panel }),
    setSidebar: (sidebarOpen) => set({ sidebarOpen }),
    openEditor: (file) => set({ editor: { file }, sidebarOpen: false }),
    closeEditor: () => set({ editor: null }),
    setDraft: (draft) => set({ draft }),

    updateSettings(mutate) {
      const current = get().settings;
      if (!current) return;
      const next = structuredClone(current);
      mutate(next);
      set({ settings: next });
      pendingSave = next;
      window.clearTimeout(saveTimer);
      saveTimer = window.setTimeout(() => void get().flushSettings(), 400);
    },
    replaceSettings(settings) {
      pendingSave = null;
      window.clearTimeout(saveTimer);
      set({ settings });
    },
    async flushSettings() {
      window.clearTimeout(saveTimer);
      const toSave = pendingSave;
      pendingSave = null;
      if (!toSave) return;
      try {
        await api.saveSettings(toSave);
      } catch (e) {
        get().toast(`Could not save settings: ${errorMessage(e)}`);
      }
    },
    async refreshSecrets() {
      set({ secrets: await api.getSecretStatus() });
    },
    async refreshCharacters() {
      try {
        set({ characters: await api.listCharacters() });
      } catch (e) {
        get().toast(errorMessage(e));
      }
    },

    async selectCharacter(file, chatFile) {
      if (get().streaming) await get().stop();
      try {
        const [card, chats] = await Promise.all([api.getCharacter(file), api.listChats(file)]);
        const target = chatFile && chats.some((c) => c.file === chatFile) ? chatFile : chats[0]?.file;
        const chat = target ? await api.loadChat(file, target) : await api.createChat(file);
        set({ activeFile: file, activeCard: card, chat, sidebarOpen: false, draft: '' });
        get().updateSettings((s) => {
          s.ui.last_character = file;
          s.ui.last_chat = chat.file;
        });
      } catch (e) {
        get().toast(errorMessage(e));
      }
    },

    async openChat(chatFile) {
      const file = get().activeFile;
      if (!file) return;
      if (get().streaming) await get().stop();
      try {
        const chat = await api.loadChat(file, chatFile);
        set({ chat, panel: null });
        get().updateSettings((s) => {
          s.ui.last_chat = chat.file;
        });
      } catch (e) {
        get().toast(errorMessage(e));
      }
    },

    async newChat() {
      const file = get().activeFile;
      if (!file) return;
      if (get().streaming) await get().stop();
      try {
        const chat = await api.createChat(file);
        set({ chat, panel: null });
        get().updateSettings((s) => {
          s.ui.last_chat = chat.file;
        });
      } catch (e) {
        get().toast(errorMessage(e));
      }
    },

    async characterSaved(file) {
      await get().refreshCharacters();
      if (get().activeFile === file) {
        set({ activeCard: await api.getCharacter(file) });
      } else {
        await get().selectCharacter(file);
      }
    },

    async characterDeleted(file) {
      await get().refreshCharacters();
      if (get().activeFile === file) {
        set({ activeFile: null, activeCard: null, chat: null });
        get().updateSettings((s) => {
          s.ui.last_character = null;
          s.ui.last_chat = null;
        });
      }
    },

    async send(text) {
      const { chat, settings, streaming } = get();
      if (!chat || !settings || streaming) return;
      const trimmed = text.trim();
      if (trimmed) {
        const message: ChatMessage = {
          name: activePersona(settings).name,
          is_user: true,
          is_system: false,
          send_date: sendDateNow(),
          mes: trimmed,
          extra: {},
        };
        set({ draft: '' });
        setMessages([...chat.messages, message]);
        await persist();
      }
      await get().generate('normal');
    },

    async generate(kind) {
      const { chat, activeFile, activeCard, streaming } = get();
      if (!chat || !activeFile || !activeCard || streaming) return;
      await get().flushSettings();
      let messages = chat.messages;
      const last = messages[messages.length - 1];
      let history = messages;
      let index = messages.length - 1;
      let base = '';

      if (kind === 'regenerate') {
        if (!last || last.is_user) return get().generate('normal');
        history = messages.slice(0, -1);
        const swipes = [...(last.swipes ?? [last.mes]), ''];
        messages = [
          ...history,
          {
            ...last,
            swipes,
            swipe_id: swipes.length - 1,
            mes: '',
            extra: { ...(last.extra ?? {}), reasoning: undefined },
          },
        ];
      } else if (kind === 'continue') {
        if (!last || last.is_user) {
          get().toast('The last message must be from the character to continue it.', 'info');
          return;
        }
        base = last.mes;
      } else if (kind === 'impersonate') {
        index = -1;
      } else {
        messages = [
          ...messages,
          {
            name: activeCard.name,
            is_user: false,
            is_system: false,
            send_date: sendDateNow(),
            mes: '',
            swipes: [''],
            swipe_id: 0,
            swipe_info: [],
            extra: {},
          },
        ];
        index = messages.length - 1;
      }

      const genId = newId();
      const mode: GenMode = kind === 'continue' || kind === 'impersonate' ? kind : 'normal';
      set({ chat: { ...chat, messages }, streaming: { genId, kind, index, base, text: '', reasoning: '' } });
      let failure: string | null = null;
      try {
        await api.generate({ gen_id: genId, char_file: activeFile, history, mode }, (event) => {
          const s = get().streaming;
          if (!s || s.genId !== genId) return;
          set({
            streaming:
              event.type === 'delta'
                ? { ...s, text: s.text + event.text }
                : { ...s, reasoning: s.reasoning + event.text },
          });
        });
      } catch (e) {
        failure = errorMessage(e);
      }
      await finishGeneration(genId, failure);
    },

    async stop() {
      const s = get().streaming;
      if (s) await api.stopGeneration(s.genId).catch(() => false);
    },

    async editMessage(index, text) {
      const chat = get().chat;
      if (!chat?.messages[index]) return;
      const messages = [...chat.messages];
      messages[index] = withText(messages[index], text);
      setMessages(messages);
      await persist();
    },

    async deleteMessage(index) {
      const chat = get().chat;
      if (!chat || get().streaming) return;
      setMessages(chat.messages.filter((_, i) => i !== index));
      await persist();
    },

    async swipe(index, direction) {
      const chat = get().chat;
      const message = chat?.messages[index];
      if (!chat || !message || message.is_user || get().streaming) return;
      const swipes = message.swipes ?? [message.mes];
      const id = (message.swipe_id ?? 0) + direction;
      if (id < 0) return;
      if (id >= swipes.length) {
        if (index === chat.messages.length - 1) await get().generate('regenerate');
        return;
      }
      const messages = [...chat.messages];
      messages[index] = {
        ...message,
        swipes,
        swipe_id: id,
        mes: swipes[id],
        extra: { ...(message.extra ?? {}), reasoning: swipeReasoning(message, id) },
      };
      setMessages(messages);
      await persist();
    },
  };
});
