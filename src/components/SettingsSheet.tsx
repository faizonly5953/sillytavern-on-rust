// Settings: connection, generation, prompts, personas, appearance.

import { useEffect, useRef, useState } from 'react';
import { Cloud, CloudOff, ImagePlus, Plus, RefreshCw, Trash2 } from 'lucide-react';
import { api } from '../lib/api';
import { activePersona, useApp } from '../lib/store';
import type { Provider, Settings } from '../lib/types';
import { cn, errorMessage, formatDate, newId, personaAvatar, readFileBase64 } from '../lib/utils';
import { Avatar, Button, Field, Input, Modal, Segmented, Slider, Textarea, Toggle } from './ui';

type Tab = 'connection' | 'generation' | 'prompts' | 'persona' | 'cloud' | 'appearance';
type Update = (mutate: (draft: Settings) => void) => void;

const OPENAI_PRESETS: [string, string][] = [
  ['OpenAI', 'https://api.openai.com/v1'],
  ['OpenRouter', 'https://openrouter.ai/api/v1'],
  ['DeepSeek', 'https://api.deepseek.com/v1'],
  ['Groq', 'https://api.groq.com/openai/v1'],
  ['Ollama', 'http://localhost:11434/v1'],
  ['LM Studio', 'http://localhost:1234/v1'],
  ['KoboldCpp', 'http://localhost:5001/v1'],
  ['llama.cpp', 'http://localhost:8080/v1'],
];

function ConnectionTab({ settings, update }: { settings: Settings; update: Update }) {
  const secrets = useApp((s) => s.secrets);
  const refreshSecrets = useApp((s) => s.refreshSecrets);
  const flushSettings = useApp((s) => s.flushSettings);
  const toast = useApp((s) => s.toast);
  const provider = settings.provider;
  const conn = settings.connections[provider];
  const [key, setKey] = useState('');
  const [models, setModels] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setKey('');
    setModels([]);
  }, [provider]);

  async function saveKey(value: string) {
    try {
      await api.setSecret(provider, value);
      setKey('');
      await refreshSecrets();
      toast(value ? 'API key saved' : 'API key removed', 'info');
    } catch (e) {
      toast(errorMessage(e));
    }
  }

  async function fetchModels() {
    setLoading(true);
    try {
      await flushSettings();
      const list = await api.listModels(provider);
      setModels(list);
      toast(`Connected — ${list.length} models`, 'info');
    } catch (e) {
      toast(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-5">
      <Segmented<Provider>
        value={provider}
        onChange={(v) => update((s) => void (s.provider = v))}
        options={[
          { value: 'openai', label: 'OpenAI-compatible' },
          { value: 'claude', label: 'Claude' },
          { value: 'gemini', label: 'Gemini' },
        ]}
      />
      {provider === 'openai' && (
        <div className="flex flex-wrap gap-1.5">
          {OPENAI_PRESETS.map(([label, url]) => (
            <button
              key={label}
              type="button"
              onClick={() => update((s) => void (s.connections.openai.base_url = url))}
              className={cn(
                'rounded-full border px-2.5 py-1 text-xs transition',
                conn.base_url === url ? 'border-accent/60 bg-accent/10 text-fg' : 'border-border text-muted hover:text-fg',
              )}
            >
              {label}
            </button>
          ))}
        </div>
      )}
      <Field label="Base URL">
        <Input
          value={conn.base_url}
          onChange={(e) => update((s) => void (s.connections[provider].base_url = e.target.value))}
          spellCheck={false}
        />
      </Field>
      <Field label="API key" hint={secrets[provider] ? `Saved: ${secrets[provider]}` : 'Not set. Stored only on this device.'}>
        <div className="flex gap-2">
          <Input
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={provider === 'openai' ? 'Optional for local servers' : 'Paste your key'}
            autoComplete="off"
          />
          <Button onClick={() => void saveKey(key)} disabled={!key.trim()}>
            Save
          </Button>
          {secrets[provider] && (
            <Button variant="ghost" onClick={() => void saveKey('')}>
              Clear
            </Button>
          )}
        </div>
      </Field>
      <Field label="Model">
        <div className="flex gap-2">
          {models.length > 0 ? (
            <select
              value={conn.model}
              onChange={(e) => update((s) => void (s.connections[provider].model = e.target.value))}
              className="h-9 w-full rounded-lg border border-border bg-bg px-2 text-sm"
            >
              {!models.includes(conn.model) && <option value={conn.model}>{conn.model || 'Select a model'}</option>}
              {models.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          ) : (
            <Input
              value={conn.model}
              onChange={(e) => update((s) => void (s.connections[provider].model = e.target.value))}
              placeholder="model id"
              spellCheck={false}
            />
          )}
          <Button onClick={() => void fetchModels()} disabled={loading}>
            {loading ? 'Loading…' : 'Fetch models'}
          </Button>
        </div>
      </Field>
    </div>
  );
}

function GenerationTab({ settings, update }: { settings: Settings; update: Update }) {
  const g = settings.generation;
  const set = <K extends keyof Settings['generation']>(key: K, value: Settings['generation'][K]) =>
    update((s) => void (s.generation[key] = value));
  return (
    <div className="space-y-5">
      <Slider label="Temperature" value={g.temperature} min={0} max={2} step={0.01} onChange={(v) => set('temperature', v)} />
      <Slider label="Top P" value={g.top_p} min={0} max={1} step={0.01} onChange={(v) => set('top_p', v)} />
      <Slider label="Top K" value={g.top_k} min={0} max={200} step={1} onChange={(v) => set('top_k', v)} hint="0 disables it." />
      <Slider label="Response length" unit="tokens" value={g.max_tokens} min={16} max={8192} step={16} onChange={(v) => set('max_tokens', v)} />
      <Slider
        label="Context size"
        unit="tokens"
        value={g.context_size}
        min={1024}
        max={262144}
        step={1024}
        onChange={(v) => set('context_size', v)}
        hint="Older messages are dropped to fit context minus response length."
      />
      <Slider label="Frequency penalty" value={g.frequency_penalty} min={-2} max={2} step={0.01} onChange={(v) => set('frequency_penalty', v)} hint="OpenAI-compatible only." />
      <Slider label="Presence penalty" value={g.presence_penalty} min={-2} max={2} step={0.01} onChange={(v) => set('presence_penalty', v)} hint="OpenAI-compatible only." />
      <Toggle label="Stream replies" checked={g.stream} onChange={(v) => set('stream', v)} />
    </div>
  );
}

function PromptsTab({ settings, update }: { settings: Settings; update: Update }) {
  const p = settings.prompts;
  const text = (key: keyof Settings['prompts'], label: string, hint?: string, rows = 3) => (
    <Field label={label} hint={hint}>
      <Textarea
        rows={rows}
        value={String(p[key])}
        onChange={(e) => update((s) => void ((s.prompts[key] as string) = e.target.value))}
      />
    </Field>
  );
  return (
    <div className="space-y-5">
      {text('main_prompt', 'Main prompt', 'Macros like {{char}} and {{user}} work everywhere.', 4)}
      {text('post_history', 'Post-history instructions', 'Sent after the chat history.')}
      <Toggle
        label="Prefer the character's own system prompt"
        checked={p.prefer_char_prompt}
        onChange={(v) => update((s) => void (s.prompts.prefer_char_prompt = v))}
      />
      <Toggle
        label="Prefer the character's post-history instructions"
        checked={p.prefer_char_instructions}
        onChange={(v) => update((s) => void (s.prompts.prefer_char_instructions = v))}
      />
      {text('assistant_prefill', 'Assistant prefill (Claude)', 'The reply is forced to start with this text.', 2)}
      {text('impersonation_prompt', 'Impersonation prompt')}
      {text('continue_nudge', 'Continue nudge', undefined, 2)}
      {text('improve_prompt', 'Improve prompt', 'Used by "Improve my draft". Your draft is added after this text.', 4)}
      {text('new_chat_prompt', 'New chat marker', undefined, 1)}
      {text('new_example_chat', 'Example chat marker', undefined, 1)}
    </div>
  );
}

function PersonaTab({ settings, update }: { settings: Settings; update: Update }) {
  const replaceSettings = useApp((s) => s.replaceSettings);
  const flushSettings = useApp((s) => s.flushSettings);
  const toast = useApp((s) => s.toast);
  const [version, setVersion] = useState(0);
  const fileInput = useRef<HTMLInputElement>(null);
  const persona = activePersona(settings);
  const index = settings.personas.findIndex((p) => p.id === persona.id);

  async function uploadAvatar(file: File | undefined) {
    if (!file) return;
    try {
      await flushSettings();
      replaceSettings(await api.setPersonaAvatar(persona.id, await readFileBase64(file)));
      setVersion((v) => v + 1);
    } catch (e) {
      toast(errorMessage(e));
    } finally {
      if (fileInput.current) fileInput.current.value = '';
    }
  }

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap gap-2">
        {settings.personas.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => update((s) => void (s.active_persona = p.id))}
            title={p.alias ? p.name : undefined}
            className={cn(
              'flex items-center gap-2 rounded-full border py-1 pl-1 pr-3 text-sm transition',
              p.id === persona.id ? 'border-accent/60 bg-accent/10' : 'border-border text-muted hover:text-fg',
            )}
          >
            <Avatar src={p.avatar ? personaAvatar(p.avatar, version) : null} name={p.name} className="h-6 w-6 rounded-full text-[10px]" />
            {p.alias || p.name || 'Unnamed'}
          </button>
        ))}
        <Button
          variant="ghost"
          onClick={() =>
            update((s) => {
              const id = newId();
              s.personas.push({ id, name: 'New persona', alias: '', description: '', avatar: null });
              s.active_persona = id;
            })
          }
        >
          <Plus size={15} /> Add
        </Button>
      </div>

      <div className="flex items-center gap-4">
        <Avatar src={persona.avatar ? personaAvatar(persona.avatar, version) : null} name={persona.name} className="h-16 w-16 rounded-full text-xl" />
        <Button onClick={() => fileInput.current?.click()}>
          <ImagePlus size={15} /> Avatar
        </Button>
        <input ref={fileInput} type="file" accept="image/*" hidden onChange={(e) => void uploadAvatar(e.target.files?.[0])} />
        {settings.personas.length > 1 && (
          <Button
            variant="danger"
            className="ml-auto"
            onClick={() =>
              update((s) => {
                s.personas.splice(index, 1);
                s.active_persona = s.personas[0].id;
              })
            }
          >
            <Trash2 size={15} /> Delete
          </Button>
        )}
      </div>
      <div className="grid gap-5 sm:grid-cols-2">
        <Field label="Name" hint="Used for {{user}}.">
          <Input value={persona.name} onChange={(e) => update((s) => void (s.personas[index].name = e.target.value))} />
        </Field>
        <Field label="Alias" hint="Only shown in the list above. Not sent to the model.">
          <Input
            value={persona.alias}
            placeholder="e.g. Lucien (fantasy)"
            onChange={(e) => update((s) => void (s.personas[index].alias = e.target.value))}
          />
        </Field>
      </div>
      <Field label="Description" hint="Sent to the model so the character knows who you are.">
        <Textarea rows={5} value={persona.description} onChange={(e) => update((s) => void (s.personas[index].description = e.target.value))} />
      </Field>
    </div>
  );
}

function CloudTab() {
  const cloud = useApp((s) => s.cloud);
  const syncing = useApp((s) => s.syncing);
  const syncError = useApp((s) => s.syncError);
  const syncNow = useApp((s) => s.syncNow);
  const setCloud = useApp((s) => s.setCloud);
  const setCloudSetup = useApp((s) => s.setCloudSetup);
  const toast = useApp((s) => s.toast);
  const [confirmOff, setConfirmOff] = useState(false);

  if (!cloud?.enabled) {
    return (
      <div className="space-y-4">
        <div className="flex items-start gap-3 rounded-xl border border-border bg-bg p-4">
          <CloudOff className="mt-0.5 shrink-0 text-muted" size={18} />
          <div className="text-sm">
            <div className="font-medium">Offline</div>
            <div className="text-xs text-muted">Characters, chats and settings stay on this device only.</div>
          </div>
        </div>
        <Button variant="primary" onClick={() => setCloudSetup('connect')}>
          <Cloud size={15} /> Connect to a cloud server
        </Button>
        <p className="text-xs text-muted">
          Run the sync server on your VPS with Docker; the steps are in <code>server/README.md</code> of the repository.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-5">
      <Field label="Server">
        <div className="flex items-center gap-2 rounded-lg border border-border bg-bg px-3 py-2 text-sm">
          <Cloud size={15} className="shrink-0 text-accent" />
          <span className="truncate">{cloud.url}</span>
        </div>
      </Field>
      <div className="grid gap-2 text-sm sm:grid-cols-2">
        <div className="rounded-lg border border-border bg-bg px-3 py-2">
          <div className="text-[11px] uppercase tracking-wide text-muted">Last sync</div>
          <div>{cloud.last_sync ? formatDate(cloud.last_sync) : 'Never'}</div>
        </div>
        <div className="rounded-lg border border-border bg-bg px-3 py-2">
          <div className="text-[11px] uppercase tracking-wide text-muted">API keys</div>
          <div>{cloud.has_passphrase ? 'Synced (encrypted)' : 'Kept on this device'}</div>
        </div>
      </div>
      {syncError && <p className="rounded-lg bg-danger/10 px-3 py-2 text-xs text-danger">{syncError}</p>}
      <div className="flex flex-wrap gap-2">
        <Button variant="primary" onClick={() => void syncNow({ manual: true })} disabled={syncing}>
          <RefreshCw size={15} className={syncing ? 'animate-spin' : ''} /> {syncing ? 'Syncing…' : 'Sync now'}
        </Button>
        <Button onClick={() => setCloudSetup('connect')}>Change server</Button>
        <Button
          variant={confirmOff ? 'danger' : 'ghost'}
          onClick={async () => {
            if (!confirmOff) {
              setConfirmOff(true);
              window.setTimeout(() => setConfirmOff(false), 3000);
              return;
            }
            try {
              setCloud(await api.cloudUseOffline());
            } catch (e) {
              toast(errorMessage(e));
            }
          }}
        >
          <CloudOff size={15} /> {confirmOff ? 'Click again to turn off' : 'Turn off sync'}
        </Button>
      </div>
      <p className="text-xs text-muted">
        Changes sync automatically a few seconds after you make them, when the app opens, and every two minutes. Local
        data is kept when you turn sync off.
      </p>
    </div>
  );
}

function AboutSection() {
  const appInfo = useApp((s) => s.appInfo);
  const checking = useApp((s) => s.checkingUpdate);
  const checkForUpdates = useApp((s) => s.checkForUpdates);
  return (
    <Field label="About" hint="Updates come from published GitHub releases of faizonly5953/sillytavern-on-rust.">
      <div className="flex items-center gap-3 rounded-lg border border-border bg-bg px-3 py-2">
        <div className="flex-1 text-sm">
          SillyTavern RS <span className="font-semibold">v{appInfo?.version ?? '?'}</span>
          <span className="text-muted"> · {appInfo?.os ?? ''}</span>
        </div>
        <Button onClick={() => void checkForUpdates(true)} disabled={checking}>
          {checking ? 'Checking…' : 'Check for updates'}
        </Button>
      </div>
    </Field>
  );
}

function AppearanceTab({ settings, update }: { settings: Settings; update: Update }) {
  const [dataDir, setDataDir] = useState('');
  useEffect(() => {
    api.getDataDir().then(setDataDir).catch(() => setDataDir(''));
  }, []);
  const ui = settings.ui;
  return (
    <div className="space-y-5">
      <Field label="Theme">
        <Segmented
          value={ui.theme}
          onChange={(v) => update((s) => void (s.ui.theme = v))}
          options={[
            { value: 'dark', label: 'Dark' },
            { value: 'light', label: 'Light' },
          ]}
        />
      </Field>
      <Slider label="Text size" value={ui.font_scale} min={0.8} max={1.4} step={0.05} onChange={(v) => update((s) => void (s.ui.font_scale = v))} />
      <Toggle label="Enter sends (desktop)" checked={ui.enter_to_send} onChange={(v) => update((s) => void (s.ui.enter_to_send = v))} />
      <Toggle label="Show model reasoning" checked={ui.show_reasoning} onChange={(v) => update((s) => void (s.ui.show_reasoning = v))} />
      {dataDir && (
        <Field label="Data folder" hint="Same layout as SillyTavern's data/default-user: copy characters/ and chats/ here to migrate.">
          <code className="block break-all rounded-lg bg-bg px-3 py-2 text-xs">{dataDir}</code>
        </Field>
      )}
      <AboutSection />
    </div>
  );
}

export function SettingsSheet() {
  const open = useApp((s) => s.panel === 'settings');
  const setPanel = useApp((s) => s.setPanel);
  const settings = useApp((s) => s.settings);
  const update = useApp((s) => s.updateSettings);
  const [tab, setTab] = useState<Tab>('connection');
  if (!settings) return null;
  const tabs: [Tab, string][] = [
    ['connection', 'Connection'],
    ['generation', 'Generation'],
    ['prompts', 'Prompts'],
    ['persona', 'Persona'],
    ['cloud', 'Cloud'],
    ['appearance', 'Appearance'],
  ];
  const props = { settings, update };
  return (
    <Modal open={open} onClose={() => setPanel(null)} title="Settings" wide>
      <div className="scrollbar-none -mx-5 -mt-4 mb-4 flex gap-1 overflow-x-auto border-b border-border px-4 py-2">
        {tabs.map(([id, label]) => (
          <button
            key={id}
            type="button"
            onClick={() => setTab(id)}
            className={cn(
              'shrink-0 rounded-lg px-3 py-1.5 text-sm transition',
              tab === id ? 'bg-elevated font-medium text-fg' : 'text-muted hover:text-fg',
            )}
          >
            {label}
          </button>
        ))}
      </div>
      {tab === 'connection' && <ConnectionTab {...props} />}
      {tab === 'generation' && <GenerationTab {...props} />}
      {tab === 'prompts' && <PromptsTab {...props} />}
      {tab === 'persona' && <PersonaTab {...props} />}
      {tab === 'cloud' && <CloudTab />}
      {tab === 'appearance' && <AppearanceTab {...props} />}
    </Modal>
  );
}
