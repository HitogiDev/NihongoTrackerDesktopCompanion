import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { BookmarkPlus, Image as ImageIcon, Send, Trash2 } from 'lucide-react';
import { Button } from '../components/ui/Button';
import { Field } from '../components/ui/Field';
import { Page, SectionTitle } from '../components/ui/Page';
import { events, ipc } from '../lib/ipc';
import { miningError } from '../lib/mining';
import { useAppStore } from '../store/useAppStore';

export default function MiningScreen() {
  const { t } = useTranslation('mining');
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const queryClient = useQueryClient();
  const [deck, setDeck] = useState(settings?.anki_deck ?? 'Default');
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ id: string; data: string } | null>(null);
  const sentences = useQuery({ queryKey: ['mined-sentences'], queryFn: ipc.minedSentences });

  useEffect(() => {
    if (settings?.anki_deck) setDeck(settings.anki_deck);
  }, [settings?.anki_deck]);
  useEffect(() => {
    const unlisten = listen(events.mined, () => {
      void queryClient.invalidateQueries({ queryKey: ['mined-sentences'] });
    });
    return () => { void unlisten.then((fn) => fn()); };
  }, [queryClient]);

  async function run(key: string, action: () => Promise<unknown>) {
    setBusy(key);
    setError(null);
    try {
      await action();
      await queryClient.invalidateQueries({ queryKey: ['mined-sentences'] });
    } catch (cause) {
      setError(miningError(cause));
    } finally {
      setBusy(null);
    }
  }

  return (
    <Page
      title={t('title')}
      description={t('subtitle')}
      actions={
        <Button variant="primary" size="sm" loading={busy === 'latest'} onClick={() => void run('latest', ipc.mineLatestLine)}>
          <BookmarkPlus className="w-4 h-4" />{t('mineLatest')}
        </Button>
      }
    >
      <div className="surface p-4 space-y-3">
        <p className="text-sm text-base-content/70">{t('shortcut')}</p>
        <Field label={t('deck')} hint={t('deckHint')}>
          {(id) => <input id={id} className="input w-full max-w-sm" value={deck} onChange={(event) => setDeck(event.target.value)} onBlur={() => {
            if (deck.trim() && deck !== settings?.anki_deck) void updateSettings({ anki_deck: deck.trim() });
          }} />}
        </Field>
        {error ? <p role="alert" className="text-sm text-error selectable">{error}</p> : null}
      </div>

      <SectionTitle>{t('saved', { count: sentences.data?.length ?? 0 })}</SectionTitle>
      {sentences.data?.length ? (
        <ul className="space-y-2">
          {sentences.data.map((sentence) => (
            <li key={sentence.id} className="surface p-3 space-y-2">
              <p lang="ja" className="selectable whitespace-pre-wrap wrap-break-word text-base">{sentence.text}</p>
              <div className="flex flex-wrap items-center gap-2 text-xs text-base-content/60">
                {sentence.media_title ? <span>{sentence.media_title}</span> : <span>{t('freeSession')}</span>}
                <span>{new Date(sentence.created_at).toLocaleString()}</span>
                {sentence.anki_note_id ? <span className="badge badge-success badge-sm">{t('exported')}</span> : null}
                {sentence.screenshot_path ? <span className="badge badge-info badge-sm">{t('hasScreenshot')}</span> : null}
              </div>
              {preview?.id === sentence.id ? <img src={`data:image/png;base64,${preview.data}`} alt={t('screenshotAlt')} className="max-h-48 rounded-field object-contain" /> : null}
              <div className="flex gap-2">
                {sentence.screenshot_path ? <Button size="sm" appearance="ghost" onClick={() => {
                  if (preview?.id === sentence.id) { setPreview(null); return; }
                  void ipc.minedScreenshot(sentence.id).then((data) => { if (data) setPreview({ id: sentence.id, data }); }).catch((cause) => setError(miningError(cause)));
                }}>
                  <ImageIcon className="w-4 h-4" />{t('preview')}
                </Button> : null}
                {!sentence.anki_note_id ? <Button size="sm" appearance="outline" loading={busy === sentence.id} onClick={() => void run(sentence.id, async () => {
                  if (deck.trim() && deck.trim() !== settings?.anki_deck) await updateSettings({ anki_deck: deck.trim() });
                  return ipc.exportMinedToAnki(sentence.id);
                })}>
                  <Send className="w-4 h-4" />{t('sendToAnki')}
                </Button> : null}
                <Button size="sm" appearance="ghost" aria-label={t('delete')} onClick={() => void run(`delete:${sentence.id}`, () => ipc.deleteMinedSentence(sentence.id))}>
                  <Trash2 className="w-4 h-4" />
                </Button>
              </div>
            </li>
          ))}
        </ul>
      ) : <div className="surface p-6 text-sm text-base-content/60">{t('empty')}</div>}
    </Page>
  );
}
