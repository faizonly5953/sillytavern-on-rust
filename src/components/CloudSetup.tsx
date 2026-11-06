// First-run choice (offline or cloud server) and the cloud server connection form.

import { useEffect, useState } from 'react';
import { Cloud, HardDrive, PlugZap } from 'lucide-react';
import { api } from '../lib/api';
import { useApp } from '../lib/store';
import { cn, errorMessage } from '../lib/utils';
import { Button, Field, Input, Modal, Toggle } from './ui';

function buildUrl(host: string, port: string, https: boolean) {
  let h = host.trim().replace(/^https?:\/\//i, '').replace(/\/.*$/, '');
  const p = port.trim();
  if (p && p !== (https ? '443' : '80') && !h.includes(':')) h += `:${p}`;
  return `${https ? 'https' : 'http'}://${h}`;
}

function OptionCard({ icon, title, text, onClick }: { icon: React.ReactNode; title: string; text: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full items-start gap-3 rounded-xl border border-border bg-bg p-4 text-left transition hover:border-accent/60 hover:bg-accent/5"
    >
      <span className="mt-0.5 grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-elevated text-accent">{icon}</span>
      <span>
        <span className="block text-sm font-semibold">{title}</span>
        <span className="mt-0.5 block text-xs leading-relaxed text-muted">{text}</span>
      </span>
    </button>
  );
}

export function CloudSetup() {
  const mode = useApp((s) => s.cloudSetup);
  const setMode = useApp((s) => s.setCloudSetup);
  const cloud = useApp((s) => s.cloud);
  const setCloud = useApp((s) => s.setCloud);
  const syncNow = useApp((s) => s.syncNow);
  const toast = useApp((s) => s.toast);
  const [host, setHost] = useState('');
  const [port, setPort] = useState('443');
  const [https, setHttps] = useState(true);
  const [token, setToken] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [busy, setBusy] = useState<'test' | 'connect' | null>(null);
  const [tested, setTested] = useState<string | null>(null);

  useEffect(() => {
    if (mode !== 'connect') return;
    setTested(null);
    setToken('');
    setPassphrase('');
    if (cloud?.url) {
      try {
        const u = new URL(cloud.url);
        setHost(u.hostname);
        setHttps(u.protocol === 'https:');
        setPort(u.port || (u.protocol === 'https:' ? '443' : '80'));
      } catch {
        /* keep the form empty */
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode]);

  if (!mode) return null;
  const firstRun = !cloud?.initialized;
  const url = buildUrl(host, port, https);
  const close = () => setMode(null);

  async function useOffline() {
    try {
      setCloud(await api.cloudUseOffline());
      close();
    } catch (e) {
      toast(errorMessage(e));
    }
  }

  async function test() {
    setBusy('test');
    setTested(null);
    try {
      const info = await api.cloudTest(url, token);
      setTested(`Connected to sync server v${info.version}`);
    } catch (e) {
      toast(errorMessage(e));
    } finally {
      setBusy(null);
    }
  }

  async function connect() {
    setBusy('connect');
    try {
      setCloud(await api.cloudConnect(url, token, passphrase));
      close();
      await syncNow({ manual: true });
    } catch (e) {
      toast(errorMessage(e));
    } finally {
      setBusy(null);
    }
  }

  if (mode === 'choose') {
    return (
      <Modal open onClose={close} required={firstRun} title="Where should your data live?">
        <p className="mb-4 text-sm text-muted">
          You can change this any time in Settings → Cloud.
        </p>
        <div className="space-y-2.5">
          <OptionCard
            icon={<HardDrive size={18} />}
            title="Only on this device"
            text="Characters, chats and settings stay here. Works fully offline."
            onClick={() => void useOffline()}
          />
          <OptionCard
            icon={<Cloud size={18} />}
            title="Sync with my cloud server"
            text="Connect to your own SillyTavern RS server (for example on a VPS) to use the same data on every device."
            onClick={() => setMode('connect')}
          />
        </div>
      </Modal>
    );
  }

  const canSubmit = host.trim() && token.trim() && !busy;
  return (
    <Modal
      open
      onClose={close}
      required={firstRun}
      title="Connect to a cloud server"
      footer={
        <>
          <Button variant="ghost" onClick={() => (firstRun ? setMode('choose') : close())} disabled={!!busy}>
            {firstRun ? 'Back' : 'Cancel'}
          </Button>
          <div className="ml-auto flex gap-2">
            <Button onClick={() => void test()} disabled={!canSubmit}>
              <PlugZap size={15} /> {busy === 'test' ? 'Testing…' : 'Test'}
            </Button>
            <Button variant="primary" onClick={() => void connect()} disabled={!canSubmit}>
              <Cloud size={15} /> {busy === 'connect' ? 'Connecting…' : 'Connect & sync'}
            </Button>
          </div>
        </>
      }
    >
      <div className="space-y-4">
        <div className="grid grid-cols-[1fr_6rem] gap-3">
          <Field label="Server address" hint="Domain or IP of your server">
            <Input value={host} onChange={(e) => setHost(e.target.value)} placeholder="sync.example.com" spellCheck={false} autoCapitalize="off" />
          </Field>
          <Field label="Port">
            <Input value={port} onChange={(e) => setPort(e.target.value.replace(/\D/g, ''))} inputMode="numeric" />
          </Field>
        </div>
        <Toggle
          label="Use HTTPS"
          checked={https}
          onChange={(v) => {
            setHttps(v);
            if (port === (v ? '80' : '443')) setPort(v ? '443' : '80');
          }}
        />
        {!https && (
          <p className="rounded-lg bg-danger/10 px-3 py-2 text-xs text-danger">
            Without HTTPS your token and chats travel unencrypted. Only use this inside a private network or VPN.
          </p>
        )}
        <Field label="Access token" hint="Shown in the server log on first start (docker compose logs sync).">
          <Input type="password" value={token} onChange={(e) => setToken(e.target.value)} autoComplete="off" spellCheck={false} />
        </Field>
        <Field
          label="API key passphrase"
          hint="Encrypts your API keys before they leave this device. Use the same passphrase on every device. Leave empty to keep API keys local."
        >
          <Input type="password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} autoComplete="new-password" />
        </Field>
        <div className={cn('rounded-lg bg-bg px-3 py-2 text-xs', tested ? 'text-fg' : 'text-muted')}>
          {tested ?? <>Will connect to <code>{host.trim() ? url : '—'}</code></>}
        </div>
      </div>
    </Modal>
  );
}
