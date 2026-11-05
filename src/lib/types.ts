// Types mirroring the Rust structs (serde field names are kept snake_case).

export type Provider = 'openai' | 'claude' | 'gemini';
export type Role = 'system' | 'user' | 'assistant';
export type GenMode = 'normal' | 'continue' | 'impersonate';

export interface ProviderConn {
  base_url: string;
  model: string;
}

export interface GenerationParams {
  temperature: number;
  top_p: number;
  top_k: number;
  max_tokens: number;
  context_size: number;
  frequency_penalty: number;
  presence_penalty: number;
  stream: boolean;
}

export interface PromptSettings {
  main_prompt: string;
  post_history: string;
  prefer_char_prompt: boolean;
  prefer_char_instructions: boolean;
  impersonation_prompt: string;
  continue_nudge: string;
  new_chat_prompt: string;
  new_example_chat: string;
  assistant_prefill: string;
}

export interface Persona {
  id: string;
  name: string;
  description: string;
  avatar: string | null;
}

export interface UiSettings {
  theme: 'dark' | 'light';
  font_scale: number;
  last_character: string | null;
  last_chat: string | null;
  show_reasoning: boolean;
  enter_to_send: boolean;
}

export interface Settings {
  provider: Provider;
  connections: Record<Provider, ProviderConn>;
  generation: GenerationParams;
  prompts: PromptSettings;
  personas: Persona[];
  active_persona: string;
  ui: UiSettings;
}

export interface CharacterSummary {
  file: string;
  name: string;
  description: string;
  tags: string[];
  fav: boolean;
  creator: string;
  modified: number;
}

export interface CharacterData {
  name: string;
  description: string;
  personality: string;
  scenario: string;
  first_mes: string;
  mes_example: string;
  creator_notes: string;
  system_prompt: string;
  post_history_instructions: string;
  alternate_greetings: string[];
  tags: string[];
  creator: string;
  character_version: string;
  character_book?: unknown;
  extensions: Record<string, unknown>;
  [key: string]: unknown;
}

export interface SwipeInfo {
  extra?: { reasoning?: string; [key: string]: unknown };
  [key: string]: unknown;
}

export interface ChatMessage {
  name: string;
  is_user: boolean;
  is_system?: boolean;
  send_date?: string | number;
  mes: string;
  swipes?: string[];
  swipe_id?: number;
  swipe_info?: SwipeInfo[];
  extra?: { reasoning?: string; [key: string]: unknown };
  [key: string]: unknown;
}

export interface ChatFile {
  file: string;
  header: Record<string, unknown>;
  messages: ChatMessage[];
}

export interface ChatSummary {
  file: string;
  message_count: number;
  last_message: string;
  modified: number;
}

export type StreamEvent = { type: 'delta'; text: string } | { type: 'reasoning'; text: string };

export interface PromptMessage {
  role: Role;
  content: string;
  tokens: number;
}

export interface BuiltPrompt {
  messages: PromptMessage[];
  prefill: string | null;
  total_tokens: number;
  budget: number;
  history_included: number;
  history_total: number;
}

export interface AppInfo {
  version: string;
  os: string;
}

export interface ReleaseInfo {
  version: string;
  current: string;
  notes: string;
  page_url: string;
  apk_url: string | null;
  published_at: string;
}

export function emptyCharacter(): CharacterData {
  return {
    name: '',
    description: '',
    personality: '',
    scenario: '',
    first_mes: '',
    mes_example: '',
    creator_notes: '',
    system_prompt: '',
    post_history_instructions: '',
    alternate_greetings: [],
    tags: [],
    creator: '',
    character_version: '',
    extensions: {},
  };
}
