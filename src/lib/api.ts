// Typed wrappers around Tauri `invoke` for every Rust command.
// Argument names are camelCase here; Tauri maps them to the Rust snake_case params.

import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  AppInfo,
  BuiltPrompt,
  CharacterData,
  CharacterSummary,
  ChatFile,
  ChatMessage,
  ChatSummary,
  CloudStatus,
  GenMode,
  Provider,
  ReleaseInfo,
  ServerInfo,
  Settings,
  StreamEvent,
  SyncChoice,
  SyncReport,
} from './types';

export interface GenerateRequest {
  gen_id: string;
  char_file: string;
  history: ChatMessage[];
  mode: GenMode;
}

export const api = {
  getSettings: () => invoke<Settings>('get_settings'),
  saveSettings: (settings: Settings) => invoke<void>('save_settings', { settings }),
  getDataDir: () => invoke<string>('get_data_dir'),
  getSecretStatus: () => invoke<Record<Provider, string | null>>('get_secret_status'),
  setSecret: (provider: Provider, value: string) => invoke<void>('set_secret', { provider, value }),
  setPersonaAvatar: (personaId: string, image: string) =>
    invoke<Settings>('set_persona_avatar', { personaId, image }),

  listCharacters: () => invoke<CharacterSummary[]>('list_characters'),
  getCharacter: (file: string) => invoke<CharacterData>('get_character', { file }),
  createCharacter: (card: CharacterData, avatar: string | null) =>
    invoke<string>('create_character', { card, avatar }),
  updateCharacter: (file: string, card: CharacterData, avatar: string | null) =>
    invoke<void>('update_character', { file, card, avatar }),
  setFavorite: (file: string, fav: boolean) => invoke<void>('set_favorite', { file, fav }),
  deleteCharacter: (file: string, deleteChats: boolean) =>
    invoke<void>('delete_character', { file, deleteChats }),
  importCharacter: (data: string, fileName: string) => invoke<string>('import_character', { data, fileName }),

  listChats: (charFile: string) => invoke<ChatSummary[]>('list_chats', { charFile }),
  loadChat: (charFile: string, chatFile: string) => invoke<ChatFile>('load_chat', { charFile, chatFile }),
  createChat: (charFile: string) => invoke<ChatFile>('create_chat', { charFile }),
  saveChat: (charFile: string, chat: ChatFile) =>
    invoke<void>('save_chat', { charFile, chatFile: chat.file, header: chat.header, messages: chat.messages }),
  deleteChat: (charFile: string, chatFile: string) => invoke<void>('delete_chat', { charFile, chatFile }),
  renameChat: (charFile: string, chatFile: string, newName: string) =>
    invoke<string>('rename_chat', { charFile, chatFile, newName }),

  generate: (request: GenerateRequest, onEvent: (event: StreamEvent) => void) => {
    const channel = new Channel<StreamEvent>();
    channel.onmessage = onEvent;
    return invoke<{ stopped: boolean }>('generate', { request, onEvent: channel });
  },
  stopGeneration: (genId: string) => invoke<boolean>('stop_generation', { genId }),
  previewPrompt: (charFile: string, history: ChatMessage[], mode: GenMode) =>
    invoke<BuiltPrompt>('preview_prompt', { charFile, history, mode }),
  listModels: (provider: Provider) => invoke<string[]>('list_models', { provider }),

  importCharacterUrl: (url: string) => invoke<string>('import_character_url', { url }),

  cloudStatus: () => invoke<CloudStatus>('cloud_status'),
  cloudTest: (url: string, token: string) => invoke<ServerInfo>('cloud_test', { url, token }),
  cloudConnect: (url: string, token: string, passphrase: string) =>
    invoke<CloudStatus>('cloud_connect', { url, token, passphrase }),
  cloudUseOffline: () => invoke<CloudStatus>('cloud_use_offline'),
  cloudSync: (resolutions: { key: string; choice: SyncChoice }[]) => invoke<SyncReport>('cloud_sync', { resolutions }),

  appInfo: () => invoke<AppInfo>('app_info'),
  checkRelease: () => invoke<ReleaseInfo | null>('check_release'),
};
