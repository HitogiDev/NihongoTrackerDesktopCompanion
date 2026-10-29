import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Info, Wand2, X } from 'lucide-react';
import { Button } from './ui/Button';
import { Modal } from './ui/Modal';
import { ipc } from '../lib/ipc';
import type { AnimeSession, MpvSetup, Settings } from '../lib/types';

export const MPV_PLAYERS = ['mpv', 'mpv.net'];

/** Seconds without a position before the dashboard offers the setup. */
const GRACE_SECS = 5;

/**
 * mpv is on screen, the user wants the real position and mpv is not giving
 * it. On Linux a player with `mpv-mpris` reports through the media bus, so its
 * source is already `player` and this stays quiet.
 */
export function needsMpvSetup(session: AnimeSession, settings: Settings | null): boolean {
  return (
    !!settings?.use_player_position &&
    !settings.mpv_setup_dismissed &&
    MPV_PLAYERS.includes(session.player) &&
    session.source === 'elapsed' &&
    session.elapsed_secs >= GRACE_SECS
  );
}

/**
 * Button that shows what would change in `mpv.conf` and changes it only once
 * the user confirms. Writing another program's configuration is never silent.
 */
export function MpvSetupButton({ player, size = 'sm' }: { player: string; size?: 'xs' | 'sm' }) {
  const { t } = useTranslation(['settings', 'common']);
  const [plan, setPlan] = useState<MpvSetup | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const review = async () => {
    setBusy(true);
    setMessage(null);
    try {
      setPlan(await ipc.mpvSetupPlan(player));
    } catch (cause) {
      setMessage({ ok: false, text: String(cause) });
    } finally {
      setBusy(false);
    }
  };

  const apply = async () => {
    setBusy(true);
    try {
      const done = await ipc.mpvSetupApply(player);
      setMessage({ ok: true, text: t(`mpv.done.${done.action}`, { path: done.ipc_path }) });
      setPlan(null);
    } catch (cause) {
      setMessage({ ok: false, text: String(cause) });
      setPlan(null);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-1">
      <Button size={size} appearance="outline" loading={busy && !plan} onClick={() => void review()}>
        <Wand2 className="h-4 w-4" />
        {t('mpv.configure', { player })}
      </Button>
      {message ? (
        <p className={`text-xs break-all ${message.ok ? 'text-success' : 'text-error'}`}>{message.text}</p>
      ) : null}

      <Modal
        open={plan !== null}
        onClose={() => !busy && setPlan(null)}
        title={t('mpv.confirmTitle', { player })}
        actions={
          plan?.action === 'nothing' ? (
            <Button onClick={() => setPlan(null)}>{t('common:close')}</Button>
          ) : (
            <>
              <Button appearance="ghost" disabled={busy} onClick={() => setPlan(null)}>
                {t('common:cancel')}
              </Button>
              <Button variant="primary" loading={busy} onClick={() => void apply()}>
                {t(plan?.action === 'adopt' ? 'mpv.adopt' : 'mpv.apply')}
              </Button>
            </>
          )
        }
      >
        {plan ? <PlanBody plan={plan} /> : null}
      </Modal>
    </div>
  );
}

function PlanBody({ plan }: { plan: MpvSetup }) {
  const { t } = useTranslation('settings');
  const code = 'block rounded-field bg-base-200 px-2 py-1 font-mono text-xs break-all';
  return (
    <div className="space-y-2 text-sm">
      {plan.action === 'add' ? (
        <>
          <p>{t(plan.exists ? 'mpv.addBody' : 'mpv.createBody')}</p>
          <code className={code}>{plan.conf_path}</code>
          <code className={code}>input-ipc-server={plan.ipc_path}</code>
          {plan.exists ? <p className="text-xs text-base-content/60">{t('mpv.backupNote')}</p> : null}
          <p className="text-xs text-base-content/60">{t('mpv.restartNote')}</p>
        </>
      ) : (
        <>
          <p>{t(plan.action === 'adopt' ? 'mpv.adoptBody' : 'mpv.nothingBody')}</p>
          <code className={code}>{plan.conf_path}</code>
          <code className={code}>input-ipc-server={plan.existing_ipc}</code>
        </>
      )}
    </div>
  );
}

/** Settings → Tracking: the same setup on demand, for either mpv build. */
export function MpvSetupSettings({ current }: { current?: string }) {
  const { t } = useTranslation('settings');
  const [player, setPlayer] = useState(current && MPV_PLAYERS.includes(current) ? current : 'mpv');
  return (
    <div className="flex flex-wrap items-start gap-3 p-3">
      <div className="min-w-0 flex-1">
        <p className="font-medium">{t('mpv.configure', { player })}</p>
        <p className="text-xs text-base-content/60">{t('mpv.configureHint')}</p>
      </div>
      <select className="select select-sm w-auto" aria-label={t('mpv.player')} value={player} onChange={(e) => setPlayer(e.target.value)}>
        {MPV_PLAYERS.map((name) => <option key={name} value={name}>{name}</option>)}
      </select>
      <MpvSetupButton key={player} player={player} />
    </div>
  );
}

/** Dashboard nudge under the anime card while mpv is not reporting. */
export function MpvSetupPrompt({ player, onDismiss }: { player: string; onDismiss: () => void }) {
  const { t } = useTranslation('settings');
  return (
    <div className="alert alert-info items-start py-2">
      <Info className="w-4 h-4 mt-0.5" />
      <div className="space-y-2">
        <p className="text-xs">{t('mpv.prompt', { player })}</p>
        <MpvSetupButton player={player} size="xs" />
      </div>
      <button type="button" className="btn btn-ghost btn-xs btn-square" aria-label={t('mpv.dismiss')} title={t('mpv.dismiss')} onClick={onDismiss}>
        <X className="w-3.5 h-3.5" />
      </button>
    </div>
  );
}
