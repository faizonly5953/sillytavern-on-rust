import { useEffect } from 'react';
import { CharacterEditor } from './components/CharacterEditor';
import { ChatsPanel } from './components/ChatsPanel';
import { ChatView } from './components/ChatView';
import { CloudSetup } from './components/CloudSetup';
import { ConflictDialog } from './components/ConflictDialog';
import { ImportUrlDialog } from './components/ImportUrlDialog';
import { PromptInspector } from './components/PromptInspector';
import { SettingsSheet } from './components/SettingsSheet';
import { Home } from './components/Home';
import { Toasts } from './components/ui';
import { UpdateDialog } from './components/UpdateDialog';
import { useApp } from './lib/store';
import { cn, IS_ANDROID } from './lib/utils';

export default function App() {
  const ready = useApp((s) => s.ready);
  const init = useApp((s) => s.init);
  const theme = useApp((s) => s.settings?.ui.theme ?? 'dark');
  const fontScale = useApp((s) => s.settings?.ui.font_scale ?? 1);
  // Phones show one screen at a time; desktop shows the list and the chat side by side.
  const chatOpen = useApp((s) => s.view === 'chat' && s.activeFile !== null);
  // Only with a chat on screen: the chat header holds the button that brings the list back.
  const sidebarHidden = useApp((s) => !IS_ANDROID && s.sidebarHidden && s.chat !== null && s.activeCard !== null);

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
      {/* Desktop: the width animates to 0 while the inner box keeps its size, so the list slides away instead of reflowing. */}
      <aside
        inert={sidebarHidden}
        className={cn(
          'h-full w-full md:block md:shrink-0 md:overflow-hidden md:transition-[width] md:duration-300 md:ease-[cubic-bezier(0.2,0.8,0.2,1)]',
          sidebarHidden ? 'md:w-0' : 'md:w-[360px]',
          chatOpen ? 'hidden' : 'animate-fade block md:animate-none',
        )}
      >
        <div className="h-full w-full md:w-[360px] md:border-r md:border-border">
          <Home />
        </div>
      </aside>
      <main
        className={cn('min-w-0 flex-1 flex-col md:flex', chatOpen ? 'animate-slide flex md:animate-none' : 'hidden')}
      >
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
