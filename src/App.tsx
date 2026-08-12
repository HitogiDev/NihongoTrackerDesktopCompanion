import { useEffect } from 'react';
import { Navigate, Route, Routes, useNavigate } from 'react-router-dom';
import { listen } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Sidebar } from './components/Sidebar';
import { PageLoader } from './components/ui/Spinner';
import { useAppStore } from './store/useAppStore';
import { events } from './lib/ipc';
import Dashboard from './screens/Dashboard';
import Queue from './screens/Queue';
import LibraryScreen from './screens/Library';
import TexthookerScreen from './screens/Texthooker';
import SettingsScreen from './screens/Settings';
import Onboarding from './screens/Onboarding';

interface VnStartedPayload {
  contentId: string;
  title: string;
  type: string;
  image: string | null;
  openTexthooker: boolean;
  externalUrl: string | null;
}

export default function App() {
  const ready = useAppStore((s) => s.ready);
  const settings = useAppStore((s) => s.settings);
  const bootstrap = useAppStore((s) => s.bootstrap);
  const navigate = useNavigate();

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  // A detected visual novel opens its texthooker room automatically — the whole
  // point of the desktop companion.
  useEffect(() => {
    const unlisten = listen<VnStartedPayload>(events.vnStarted, async (event) => {
      // The watcher already pointed the hooker at this work and connected it;
      // the window only has to show the right screen.
      const { openTexthooker, externalUrl } = event.payload;
      if (openTexthooker) navigate('/texthooker');
      if (externalUrl) {
        await openUrl(externalUrl).catch(() => undefined);
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [navigate]);

  if (!ready || !settings) {
    return (
      <div className="min-h-screen bg-base-200">
        <PageLoader />
      </div>
    );
  }

  const configured = Boolean(settings.api_key && settings.api_key.trim());

  return (
    <div className="flex h-screen bg-base-200 text-base-content overflow-hidden">
      {configured ? <Sidebar /> : null}
      <main className="flex-1 min-w-0 overflow-y-auto">
        <Routes>
          {!configured ? (
            <>
              <Route path="/onboarding" element={<Onboarding />} />
              <Route path="*" element={<Navigate to="/onboarding" replace />} />
            </>
          ) : (
            <>
              <Route path="/" element={<Dashboard />} />
              <Route path="/queue" element={<Queue />} />
              <Route path="/library" element={<LibraryScreen />} />
              <Route path="/texthooker" element={<TexthookerScreen />} />
              <Route path="/settings" element={<SettingsScreen />} />
              <Route path="/onboarding" element={<Onboarding />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </>
          )}
        </Routes>
      </main>
    </div>
  );
}
