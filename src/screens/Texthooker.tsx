import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import {
  BookmarkCheck,
  Eraser,
  Link2,
  Pause,
  Play,
  Plug,
  PlugZap,
  RotateCcw,
  Settings2,
  Type,
} from 'lucide-react';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/Empty';
import { Modal } from '../components/ui/Modal';
import { Field } from '../components/ui/Field';
import { MediaSearchModal } from '../components/MediaSearchModal';
import { formatClock, numberWithCommas, readingSpeed } from '../lib/format';
import { events, ipc } from '../lib/ipc';
import { useAppStore } from '../store/useAppStore';
import type { HookerTotals, TextLine } from '../lib/types';

const LOCAL_SESSION = 'local';

const FONTS = [
  'Noto Sans JP',
  'Noto Serif JP',
  'Yu Gothic',
  'Meiryo',
  'MS Gothic',
  'Inter Variable',
];

export default function TexthookerScreen() {
  const queryClient = useQueryClient();
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const hooker = useAppStore((s) => s.hooker);
  const tracker = useAppStore((s) => s.tracker);

  const contentId = hooker?.content_id ?? LOCAL_SESSION;
  const [lines, setLines] = useState<TextLine[]>([]);
  const [totals, setTotals] = useState<HookerTotals | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [attaching, setAttaching] = useState(false);
  const [manualSeconds, setManualSeconds] = useState(0);
  const [manualRunning, setManualRunning] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const anchorRef = useRef({ at: Date.now(), seconds: 0 });

  // The watcher owns the clock whenever the matching VN is running.
  const autoSession =
    tracker?.vn && tracker.vn.content_id === contentId ? tracker.vn : null;
  const seconds = autoSession ? autoSession.elapsed_secs : manualSeconds;

  const reload = useCallback(async () => {
    const [nextLines, nextTotals] = await Promise.all([
      ipc.hookerLines(contentId, 500),
      ipc.hookerTotals(contentId),
    ]);
    setLines(nextLines);
    setTotals(nextTotals);
    if (!autoSession) {
      setManualSeconds(nextTotals.timerSeconds);
      anchorRef.current = { at: Date.now(), seconds: nextTotals.timerSeconds };
    }
  }, [contentId, autoSession]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    const unlisten = listen<{ line: TextLine; totals: { lines: number; chars: number } }>(
      events.hookerLine,
      (event) => {
        if (event.payload.line.content_id !== contentId) return;
        setLines((prev) => [...prev, event.payload.line]);
        setTotals((prev) =>
          prev
            ? {
                ...prev,
                lines: event.payload.totals.lines,
                chars: event.payload.totals.chars,
                sessionChars: Math.max(0, event.payload.totals.chars - prev.baselineChars),
                sessionLines: Math.max(0, event.payload.totals.lines - prev.baselineLines),
              }
            : prev
        );
      }
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [contentId]);

  // Wall-clock anchored manual timer — same approach as the web texthooker, so
  // a throttled render loop never loses time.
  useEffect(() => {
    if (autoSession || !manualRunning) return;
    anchorRef.current = { at: Date.now(), seconds: manualSeconds };
    const id = setInterval(() => {
      const { at, seconds: banked } = anchorRef.current;
      setManualSeconds(banked + Math.floor((Date.now() - at) / 1000));
    }, 1000);
    return () => clearInterval(id);
    // manualSeconds intentionally omitted: re-anchoring each tick would freeze it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [manualRunning, autoSession]);

  // Persist the manual timer so a restart doesn't lose the session.
  useEffect(() => {
    if (autoSession) return;
    const id = setInterval(() => {
      void ipc.hookerSetTimer(contentId, manualSeconds);
    }, 15000);
    return () => clearInterval(id);
  }, [contentId, manualSeconds, autoSession]);

  useEffect(() => {
    const el = listRef.current;
    if (!el) return;
    if (settings?.texthooker_vertical) el.scrollLeft = 0;
    else el.scrollTop = el.scrollHeight;
  }, [lines, settings?.texthooker_vertical]);

  const sessionChars = autoSession?.session_chars ?? totals?.sessionChars ?? 0;
  const sessionLines = autoSession?.session_lines ?? totals?.sessionLines ?? 0;
  const speed = useMemo(() => readingSpeed(sessionChars, seconds), [sessionChars, seconds]);

  const connected = hooker?.status === 'connected';
  const canLog = contentId !== LOCAL_SESSION && (sessionChars > 0 || seconds >= 60);

  async function handleLog() {
    if (!totals) return;
    await ipc.logTextSession({
      content_id: contentId,
      media_type: totals.mediaType ?? 'vn',
      description: totals.mediaTitle ?? hooker?.content_title ?? 'Sesión de lectura',
      chars: sessionChars,
      seconds,
    });
    setManualSeconds(0);
    setManualRunning(false);
    anchorRef.current = { at: Date.now(), seconds: 0 };
    await reload();
    void queryClient.invalidateQueries({ queryKey: ['outbox'] });
  }

  const fontStack = `"${settings?.texthooker_font_family ?? 'Noto Sans JP'}", var(--font-jp)`;

  return (
    <div className="flex flex-col h-screen">
      <header className="shrink-0 bg-neutral text-neutral-content">
        <div className="px-4 h-14 flex items-center gap-3">
          <button
            type="button"
            className="btn btn-ghost btn-sm gap-2 max-w-[40%]"
            onClick={() => setAttaching(true)}
          >
            <Type className="w-4 h-4" />
            <span className="truncate">
              {hooker?.content_title ?? totals?.mediaTitle ?? 'Sesión libre'}
            </span>
          </button>

          <div className="flex items-center gap-4 ml-auto tabular-nums text-sm">
            <span className="font-semibold">{formatClock(seconds)}</span>
            <span className="opacity-80">{numberWithCommas(sessionChars)} car.</span>
            <span className="opacity-80">{numberWithCommas(speed)} c/h</span>
            <span className="opacity-60">{sessionLines} líneas</span>
          </div>

          <div className="flex items-center gap-1">
            {autoSession ? (
              <span
                className={`badge badge-sm ${
                  autoSession.counting ? 'badge-success' : 'badge-ghost'
                }`}
              >
                {autoSession.counting ? 'auto' : 'pausado'}
              </span>
            ) : (
              <>
                <Button
                  size="sm"
                  appearance="ghost"
                  shape="square"
                  aria-label={manualRunning ? 'Pausar' : 'Reanudar'}
                  onClick={() => setManualRunning((v) => !v)}
                >
                  {manualRunning ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4" />}
                </Button>
                <Button
                  size="sm"
                  appearance="ghost"
                  shape="square"
                  aria-label="Reiniciar cronómetro"
                  onClick={() => {
                    setManualSeconds(0);
                    anchorRef.current = { at: Date.now(), seconds: 0 };
                    void ipc.hookerSetTimer(contentId, 0);
                  }}
                >
                  <RotateCcw className="w-4 h-4" />
                </Button>
              </>
            )}
            <Button
              size="sm"
              appearance="ghost"
              shape="square"
              aria-label={connected ? 'Desconectar' : 'Conectar'}
              className={connected ? 'text-success' : ''}
              onClick={() =>
                void (connected ? ipc.hookerDisconnect() : ipc.hookerConnect())
              }
            >
              {connected ? <PlugZap className="w-4 h-4" /> : <Plug className="w-4 h-4" />}
            </Button>
            <Button
              size="sm"
              appearance="ghost"
              shape="square"
              aria-label="Ajustes"
              onClick={() => setShowSettings(true)}
            >
              <Settings2 className="w-4 h-4" />
            </Button>
          </div>
        </div>
      </header>

      <div
        ref={listRef}
        className={`flex-1 min-h-0 bg-base-100 selectable ${
          settings?.texthooker_vertical ? 'overflow-x-auto overflow-y-hidden' : 'overflow-y-auto'
        }`}
        style={{
          fontFamily: fontStack,
          fontSize: `${settings?.texthooker_font_size ?? 24}px`,
          lineHeight: settings?.texthooker_line_spacing ?? 1.5,
          writingMode: settings?.texthooker_vertical ? 'vertical-rl' : undefined,
        }}
      >
        {lines.length === 0 ? (
          <div style={{ writingMode: 'horizontal-tb', fontSize: '16px' }}>
            <EmptyState
              icon={Type}
              title={connected ? 'Esperando texto' : 'Texthooker desconectado'}
              description={
                connected
                  ? `Conectado a ${hooker?.url}. Abre la novela y el texto aparecerá aquí.`
                  : `Arranca Textractor o Agent con la extensión WebSocket en ${settings?.texthooker_ws_url}.`
              }
              action={
                !connected ? (
                  <Button variant="primary" size="sm" onClick={() => void ipc.hookerConnect()}>
                    Conectar
                  </Button>
                ) : undefined
              }
            />
          </div>
        ) : (
          <div className="p-6 flex flex-col gap-4">
            {lines.map((line) => (
              <p
                key={line.id}
                className="group relative hover:bg-base-200/50 rounded-field px-2 -mx-2 transition-colors"
              >
                {line.text}
                <button
                  type="button"
                  aria-label="Borrar línea"
                  className="absolute top-0 right-0 opacity-0 group-hover:opacity-100 btn btn-ghost btn-xs btn-circle"
                  style={{ writingMode: 'horizontal-tb' }}
                  onClick={() => {
                    void ipc.hookerDeleteLine(line.id).then(reload);
                  }}
                >
                  ×
                </button>
              </p>
            ))}
          </div>
        )}
      </div>

      <footer className="shrink-0 border-t border-base-300 bg-base-200 px-4 py-2 flex items-center gap-2">
        <Button
          size="sm"
          appearance="ghost"
          onClick={() => {
            void ipc.hookerClear(contentId).then(() => {
              setLines([]);
              void reload();
            });
          }}
        >
          <Eraser className="w-4 h-4" />
          Vaciar
        </Button>
        {contentId === LOCAL_SESSION ? (
          <Button size="sm" appearance="outline" onClick={() => setAttaching(true)}>
            <Link2 className="w-4 h-4" />
            Vincular obra
          </Button>
        ) : null}
        <span className="text-xs text-base-content/50 ml-auto">
          {totals ? `${numberWithCommas(totals.chars)} car. totales` : ''}
        </span>
        <Button variant="primary" size="sm" disabled={!canLog} onClick={() => void handleLog()}>
          <BookmarkCheck className="w-4 h-4" />
          Registrar sesión
        </Button>
      </footer>

      <MediaSearchModal
        open={attaching}
        onClose={() => setAttaching(false)}
        mediaType={totals?.mediaType ?? 'vn'}
        initialQuery={hooker?.content_title ?? ''}
        onPick={async (hit, type) => {
          await ipc.hookerAttachMedia(
            contentId,
            hit.contentId,
            hit.title.contentTitleRomaji || hit.title.contentTitleNative,
            hit.type ?? type
          );
          setAttaching(false);
          await reload();
        }}
      />

      <Modal
        open={showSettings}
        onClose={() => setShowSettings(false)}
        title="Ajustes del texthooker"
      >
        <div className="space-y-3">
          <Field label="WebSocket" hint="Textractor/Agent exponen ws://localhost:6677 por defecto.">
            {(id) => (
              <div className="flex gap-2">
                <input
                  id={id}
                  className="input flex-1 font-mono text-sm"
                  value={settings?.texthooker_ws_url ?? ''}
                  onChange={(e) => void updateSettings({ texthooker_ws_url: e.target.value })}
                />
                <Button appearance="outline" onClick={() => void ipc.hookerConnect()}>
                  Reconectar
                </Button>
              </div>
            )}
          </Field>

          <div className="grid grid-cols-2 gap-3">
            <Field label="Fuente">
              {(id) => (
                <select
                  id={id}
                  className="select w-full"
                  value={settings?.texthooker_font_family ?? 'Noto Sans JP'}
                  onChange={(e) => void updateSettings({ texthooker_font_family: e.target.value })}
                >
                  {FONTS.map((f) => (
                    <option key={f} value={f}>
                      {f}
                    </option>
                  ))}
                </select>
              )}
            </Field>
            <Field label={`Tamaño: ${settings?.texthooker_font_size ?? 24}px`}>
              {(id) => (
                <input
                  id={id}
                  type="range"
                  min={12}
                  max={72}
                  className="range range-primary"
                  value={settings?.texthooker_font_size ?? 24}
                  onChange={(e) =>
                    void updateSettings({ texthooker_font_size: Number(e.target.value) })
                  }
                />
              )}
            </Field>
          </div>

          <Field label={`Interlineado: ${settings?.texthooker_line_spacing ?? 1.5}`}>
            {(id) => (
              <input
                id={id}
                type="range"
                min={1}
                max={3}
                step={0.1}
                className="range range-primary"
                value={settings?.texthooker_line_spacing ?? 1.5}
                onChange={(e) =>
                  void updateSettings({ texthooker_line_spacing: Number(e.target.value) })
                }
              />
            )}
          </Field>

          <label className="flex items-center justify-between gap-4 rounded-field px-3 py-2.5 hover:bg-base-200 cursor-pointer">
            <span className="text-sm font-medium">Escritura vertical</span>
            <input
              type="checkbox"
              className="toggle toggle-primary"
              checked={settings?.texthooker_vertical ?? false}
              onChange={(e) => void updateSettings({ texthooker_vertical: e.target.checked })}
            />
          </label>

          <label className="flex items-center justify-between gap-4 rounded-field px-3 py-2.5 hover:bg-base-200 cursor-pointer">
            <span className="text-sm font-medium">Evitar líneas duplicadas</span>
            <input
              type="checkbox"
              className="toggle toggle-primary"
              checked={settings?.texthooker_prevent_duplicates ?? false}
              onChange={(e) =>
                void updateSettings({ texthooker_prevent_duplicates: e.target.checked })
              }
            />
          </label>
        </div>
      </Modal>
    </div>
  );
}
