import { lazy, Suspense, useEffect, useRef } from 'react';
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom';
import { listen } from '@tauri-apps/api/event';
import { Sidebar } from './components/Sidebar';
import { PageLoader } from './components/ui/Spinner';
import { useAppStore } from './store/useAppStore';
import { events, ipc } from './lib/ipc';

// Every window loads this same bundle, so each screen is its own chunk: the
// session overlay should not parse the 2,500-line texthooker to show a timer.
const LogCelebrationHost = lazy(() => import('./components/LogCelebrationHost'));
const QuickLogModal = lazy(() => import('./components/QuickLogModal').then((m) => ({ default: m.QuickLogModal })));
const Dashboard = lazy(() => import('./screens/Dashboard'));
const Queue = lazy(() => import('./screens/Queue'));
const LibraryScreen = lazy(() => import('./screens/Library'));
const MiningScreen = lazy(() => import('./screens/Mining'));
const TexthookerScreen = lazy(() => import('./screens/Texthooker'));
const SettingsScreen = lazy(() => import('./screens/Settings'));
const RankingScreen = lazy(() => import('./screens/Ranking'));
const SocialScreen = lazy(() => import('./screens/Social'));
const Onboarding = lazy(() => import('./screens/Onboarding'));
const SessionOverlay = lazy(() => import('./components/SessionOverlay').then((m) => ({ default: m.SessionOverlay })));
const ReadingOverlay = lazy(() => import('./components/ReadingOverlay').then((m) => ({ default: m.ReadingOverlay })));
const ReadingOverlayPopup = lazy(() => import('./components/ReadingOverlay').then((m) => ({ default: m.ReadingOverlayPopup })));
const ReadingOverlaySettings = lazy(() => import('./components/ReadingOverlaySettings').then((m) => ({ default: m.ReadingOverlaySettings })));
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useQuickLogStore } from './store/useQuickLogStore';

export default function App() {
  const windowLabel = getCurrentWindow().label;
  const isOverlay = windowLabel === 'overlay';
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

  // Requests from outside the window: the "streak at risk" notification and the
  // tray's log entry land on the log form, the session overlay on a screen.
  // They arrive as events while the window exists; when Rust had to build the
  // window for them, the page was not listening yet and asks for it instead.
  // (Auto-opening the texthooker for a detected novel is done by Rust: this
  // window may not exist while the app lives in the tray.)
  useEffect(() => {
    if (windowLabel !== 'main') return;
    const handle = (intent: string) => {
      if (intent === '/log:streak') openQuickLog('streak');
      else if (intent === '/log') openQuickLog('manual');
      else navigate(intent);
    };
    const offOpen = listen(events.streakOpenLog, () => openQuickLog('streak'));
    const offNavigate = listen<string>(events.overlayNavigate, (event) => handle(event.payload));
    const offAuth = listen(events.streakAuthRequired, () => setAuthRequired(true));
    void Promise.all([offOpen, offNavigate])
      .then(() => ipc.takeMainIntent())
      .then((intent) => { if (intent) handle(intent); });
    return () => {
      void offOpen.then((fn) => fn());
      void offNavigate.then((fn) => fn());
      void offAuth.then((fn) => fn());
    };
  }, [openQuickLog, setAuthRequired, windowLabel, navigate]);

  // The main window is built hidden (see `build_main_window`) and shown here,
  // once there is something themed to show, instead of flashing a blank page.
  const shown = useRef(false);
  useEffect(() => {
    if (windowLabel !== 'main' || !ready || shown.current) return;
    shown.current = true;
    requestAnimationFrame(() => {
      const current = getCurrentWindow();
      void current.show().then(() => current.setFocus());
    });
  }, [ready, windowLabel]);

  if (!ready || !settings) {
    if (windowLabel.startsWith('reading-overlay')) return null;
    return (
      <div className="min-h-screen bg-base-200">
        <PageLoader />
      </div>
    );
  }

  if (isOverlay) return <SessionOverlay />;
  if (windowLabel === 'reading-overlay') return <ReadingOverlay />;
  if (windowLabel === 'reading-overlay-popup') return <ReadingOverlayPopup />;
  if (windowLabel === 'reading-overlay-settings') {
    return <div className="h-screen overflow-y-auto bg-base-300 text-base-content">
      <div className="mx-auto max-w-3xl space-y-4 p-5 sm:p-8"><ReadingOverlaySettings /></div>
    </div>;
  }

  if (windowLabel === 'settings') {
    return <div className="h-screen overflow-y-auto bg-base-100 text-base-content"><SettingsScreen /></div>;
  }

  if (windowLabel === 'texthooker') return <TexthookerScreen />;

  const configured = Boolean(settings.api_key && settings.api_key.trim());
  // The reader takes the whole window, like the web app's chrome-less route.
  const isReader = windowLabel === 'texthooker' || location.pathname.startsWith('/texthooker');

  return (
    <div className="app-shell flex h-screen overflow-hidden bg-base-100 text-base-content">
      {configured && !isReader ? <Sidebar /> : null}
      <main className="min-w-0 flex-1 overflow-y-auto bg-base-100">
        <Suspense fallback={null}>
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
              <Route path="/mining" element={<MiningScreen />} />
              <Route path="/ranking" element={<RankingScreen />} />
              <Route path="/social" element={<SocialScreen />} />
              <Route path="/texthooker" element={<TexthookerScreen />} />
              <Route path="/settings" element={<SettingsScreen />} />
              <Route path="/onboarding" element={<Onboarding />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </>
          )}
        </Routes>
        </Suspense>
      </main>
      <Suspense fallback={null}>
        <LogCelebrationHost />
        <QuickLogModal open={quickLogOpen} onClose={closeQuickLog} reason={quickLogReason} />
      </Suspense>
    </div>
  );
}
