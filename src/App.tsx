import { useEffect } from 'react';
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom';
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
import RankingScreen from './screens/Ranking';
import Onboarding from './screens/Onboarding';
import LogCelebrationHost from './components/LogCelebrationHost';
import { QuickLogModal } from './components/QuickLogModal';
import { useQuickLogStore } from './store/useQuickLogStore';

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
  const location = useLocation();
  const quickLogOpen = useQuickLogStore((s) => s.open);
  const quickLogReason = useQuickLogStore((s) => s.reason);
  const openQuickLog = useQuickLogStore((s) => s.openForm);
  const closeQuickLog = useQuickLogStore((s) => s.closeForm);
  const setAuthRequired = useAppStore((s) => s.setAuthRequired);

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

  // Clicking the "streak at risk" notification (or the tray entry) has to land
  // on the log form wherever the user happens to be.
  useEffect(() => {
    const offOpen = listen(events.streakOpenLog, () => openQuickLog('streak'));
    const offAuth = listen(events.streakAuthRequired, () => setAuthRequired(true));
    return () => {
      void offOpen.then((fn) => fn());
      void offAuth.then((fn) => fn());
    };
  }, [openQuickLog, setAuthRequired]);

  if (!ready || !settings) {
    return (
      <div className="min-h-screen bg-base-200">
        <PageLoader />
      </div>
    );
  }

  const configured = Boolean(settings.api_key && settings.api_key.trim());
  // The reader takes the whole window, like the web app's chrome-less route.
  const isReader = location.pathname.startsWith('/texthooker');

  return (
    <div className="flex h-screen bg-base-200 text-base-content overflow-hidden">
      {configured && !isReader ? <Sidebar /> : null}
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
              <Route path="/ranking" element={<RankingScreen />} />
              <Route path="/texthooker" element={<TexthookerScreen />} />
              <Route path="/settings" element={<SettingsScreen />} />
              <Route path="/onboarding" element={<Onboarding />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </>
          )}
        </Routes>
      </main>
      <LogCelebrationHost />
      <QuickLogModal open={quickLogOpen} onClose={closeQuickLog} reason={quickLogReason} />
    </div>
  );
}
