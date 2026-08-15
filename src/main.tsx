import React from 'react';
import ReactDOM from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { HashRouter } from 'react-router-dom';
// Only the UI font is bundled. Japanese text falls back to the system face
// (Yu Gothic / Hiragino / Noto), which every target OS ships — bundling the
// ~120 Noto subsets would add several MB to the installer for no gain.
import '@fontsource-variable/inter';
import App from './App';
import { initI18n } from './i18n';
import { detectLanguage } from './i18n/languages';
import './index.css';

// Start in the system language; the store switches to the stored one as soon as
// settings come back from Rust.
initI18n(detectLanguage());

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false, staleTime: 10_000 },
  },
});

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <HashRouter>
        <App />
      </HashRouter>
    </QueryClientProvider>
  </React.StrictMode>
);
