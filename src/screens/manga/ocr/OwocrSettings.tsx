import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Check, CircleAlert, CircleCheck, Copy, PlugZap, ShieldCheck, TriangleAlert } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { Field } from '../../../components/ui/Field';
import { ipc } from '../../../lib/ipc';
import type { OcrStatus, OwocrProbe, Settings } from '../../../lib/types';
import { useAppStore } from '../../../store/useAppStore';
import { owocrCommand, parsePort } from './owocr';
import { ocrErrorText } from './useOcr';

/** The same, in `owocr_config.ini` (section `[general]`). */
const INI_KEYS = ['read_from = websocket', 'write_to = websocket', 'output_format = json'];

/** A line to copy, with its button. */
function CopyLine({ text, label }: { text: string; label: string }) {
  const { t } = useTranslation('settings');
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1500);
    return () => window.clearTimeout(timer);
  }, [copied]);
  return (
    <div className="surface-muted flex min-w-0 items-center gap-2 py-1 pl-3 pr-1">
      <code className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap py-1 font-mono text-xs" aria-label={label}>{text}</code>
      <Button
        appearance="ghost"
        size="xs"
        aria-label={copied ? t('manga.owocrCopied') : t('manga.owocrCopy')}
        title={copied ? t('manga.owocrCopied') : t('manga.owocrCopy')}
        onClick={() => void navigator.clipboard.writeText(text).then(() => setCopied(true), () => undefined)}
      >
        {copied ? <Check className="h-3.5 w-3.5 text-success" /> : <Copy className="h-3.5 w-3.5" />}
        <span className="hidden sm:inline">{copied ? t('manga.owocrCopied') : t('manga.owocrCopy')}</span>
      </Button>
    </div>
  );
}

/** What "Test connection" found, as a panel. */
function ProbeResult({ probe }: { probe: OwocrProbe }) {
  const { t } = useTranslation(['settings', 'manga']);
  if (probe.ok) {
    return (
      <div className="surface-muted flex items-start gap-3 border border-success/30 px-4 py-3">
        <CircleCheck className="mt-0.5 h-5 w-5 shrink-0 text-success" aria-hidden="true" />
        <div className="min-w-0 text-sm">
          <p className="font-semibold">{t('manga.owocrOk')}</p>
          <p className="mt-0.5 text-base-content/60">{t('manga.owocrOkDetail', { url: probe.url, ms: Number(probe.millis) })}</p>
          {!probe.line_boxes ? <p className="mt-1 text-xs text-base-content/60">{t('manga.owocrNoLineBoxes')}</p> : null}
        </div>
      </div>
    );
  }
  // Running, but its answers have no boxes: a warning, not a failure to connect.
  if (probe.code === 'owocr-no-coordinates') {
    return (
      <div role="alert" className="surface-muted flex items-start gap-3 border border-warning/40 px-4 py-3">
        <TriangleAlert className="mt-0.5 h-5 w-5 shrink-0 text-warning" aria-hidden="true" />
        <div className="min-w-0 text-sm">
          <p className="font-semibold">{t('manga.owocrNoCoordinatesTitle')}</p>
          <p className="mt-0.5 text-base-content/70">{t('manga:ocr.errors.owocr-no-coordinates')}</p>
        </div>
      </div>
    );
  }
  return (
    <div role="alert" className="surface-muted flex items-start gap-3 border border-error/30 px-4 py-3">
      <CircleAlert className="mt-0.5 h-5 w-5 shrink-0 text-error" aria-hidden="true" />
      <div className="min-w-0 text-sm">
        <p className="font-semibold">{t('manga.owocrFailed', { url: probe.url })}</p>
        <p className="mt-0.5 text-base-content/70">{ocrErrorText(probe.code)}</p>
      </div>
    </div>
  );
}

/**
 * owocr as the OCR: how to start it (to copy), its port, a connection test
 * and the last error. The app never installs or starts it.
 */
export function OwocrSettings({ settings, status }: { settings: Settings; status: OcrStatus | undefined }) {
  const { t } = useTranslation(['settings', 'manga']);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const saved = Number(settings.manga_owocr_port);
  const [draft, setDraft] = useState(String(saved));
  const [testing, setTesting] = useState(false);
  const [probe, setProbe] = useState<OwocrProbe | null>(null);
  useEffect(() => setDraft(String(saved)), [saved]);
  const port = parsePort(draft);
  const command = owocrCommand(port ?? saved);
  const lastError = status?.engine === 'owocr' && status.error?.startsWith('owocr-') ? status.error : null;

  const test = async () => {
    if (port === null) return;
    setTesting(true);
    setProbe(null);
    try {
      if (port !== saved) await updateSettings({ manga_owocr_port: port });
      setProbe(await ipc.mangaOwocrTest(port));
    } catch (error) {
      setProbe({ url: `ws://127.0.0.1:${port}`, ok: false, code: String(error), line_boxes: false, millis: 0 });
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="space-y-3">
      <p className="text-sm text-base-content/70">{t('manga.owocrIntro')}</p>
      <div className="space-y-1.5">
        <p className="text-xs font-medium text-base-content/60">{t('manga.owocrCommand')}</p>
        <CopyLine text={command} label={t('manga.owocrCommand')} />
        <p className="text-xs leading-relaxed text-base-content/60">
          {t('manga.owocrConfig')}{' '}
          {INI_KEYS.map((line) => <code key={line} className="mr-1 inline-block rounded bg-base-200 px-1.5 py-0.5 font-mono text-[11px] text-base-content/80">{line}</code>)}
        </p>
        <p className="flex items-start gap-1.5 text-xs text-base-content/60">
          <TriangleAlert className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning" aria-hidden="true" />
          {t('manga.owocrJsonNote')}
        </p>
      </div>
      <form
        className="flex flex-wrap items-end gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          void test();
        }}
      >
        <Field label={t('manga.owocrPort')} error={port === null ? t('manga:ocr.errors.bad-port') : null}>
          {(id) => (
            <label className="input w-48" htmlFor={id}>
              <span className="font-mono text-xs text-base-content/50">ws://127.0.0.1:</span>
              <input
                id={id}
                inputMode="numeric"
                className="font-mono"
                value={draft}
                onChange={(event) => {
                  setDraft(event.target.value.replace(/[^0-9]/g, '').slice(0, 5));
                  setProbe(null);
                }}
                onBlur={() => {
                  if (port !== null && port !== saved) void updateSettings({ manga_owocr_port: port });
                }}
              />
            </label>
          )}
        </Field>
        <div className="fieldset">
          <Button type="submit" appearance="outline" size="md" loading={testing} disabled={port === null}>
            {testing ? null : <PlugZap className="h-4 w-4" />}
            {testing ? t('manga.owocrTesting') : t('manga.owocrTest')}
          </Button>
        </div>
      </form>
      <div aria-live="polite">
        {probe ? (
          <ProbeResult probe={probe} />
        ) : lastError ? (
          <p role="alert" className="text-xs text-error">{t('manga.owocrLastError', { error: ocrErrorText(lastError) })}</p>
        ) : (
          <p className="flex items-start gap-2 text-xs text-base-content/55">
            <ShieldCheck className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
            {t('manga.owocrPrivacy')}
          </p>
        )}
      </div>
    </div>
  );
}
