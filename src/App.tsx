import { useEffect } from 'react';
import { CharacterEditor } from './components/CharacterEditor';
import { ChatsPanel } from './components/ChatsPanel';
import { ChatView } from './components/ChatView';
import { CloudSetup } from './components/CloudSetup';
import { ConflictDialog } from './components/ConflictDialog';
import { ImportUrlDialog } from './components/ImportUrlDialog';
import { PromptInspector } from './components/PromptInspector';
import { SettingsSheet } from './components/SettingsSheet';
import { Sidebar } from './components/Sidebar';
import { Toasts } from './components/ui';
import { UpdateDialog } from './components/UpdateDialog';
import { useApp } from './lib/store';
import { cn } from './lib/utils';

export default function App() {
  const ready = useApp((s) => s.ready);
  const init = useApp((s) => s.init);
  const theme = useApp((s) => s.settings?.ui.theme ?? 'dark');
  const fontScale = useApp((s) => s.settings?.ui.font_scale ?? 1);
  const sidebarOpen = useApp((s) => s.sidebarOpen);
  const setSidebar = useApp((s) => s.setSidebar);

  useEffect(() => {
    void init();
  }, [init]);

  // Files dropped outside the sidebar must not make the webview navigate to them.
  useEffect(() => {
    const block = (e: DragEvent) => e.preventDefault();
    window.addEventListener('dragover', block);
    window.addEventListener('drop', block);
    return () => {
      window.removeEventListener('dragover', block);
      window.removeEventListener('drop', block);
    };
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.setProperty('--font-scale', String(fontScale));
  }, [theme, fontScale]);

  if (!ready) {
    return <div className="grid h-full place-items-center text-sm text-muted">Loading…</div>;
  }

  return (
    <div className="flex h-full overflow-hidden">
      <div
        className={cn('fixed inset-0 z-30 bg-black/50 transition-opacity md:hidden', sidebarOpen ? 'opacity-100' : 'pointer-events-none opacity-0')}
        onClick={() => setSidebar(false)}
      />
      <aside
        className={cn(
          'fixed inset-y-0 left-0 z-40 w-[85vw] max-w-[320px] transition-transform duration-200 md:static md:z-auto md:w-[300px] md:translate-x-0',
          sidebarOpen ? 'translate-x-0' : '-translate-x-full',
        )}
      >
        <Sidebar />
      </aside>
      <main className="flex min-w-0 flex-1 flex-col">
        <ChatView />
      </main>
      <SettingsSheet />
      <ChatsPanel />
      <PromptInspector />
      <CharacterEditor />
      <ImportUrlDialog />
      <UpdateDialog />
      <CloudSetup />
      <ConflictDialog />
      <Toasts />
    </div>
  );
}
