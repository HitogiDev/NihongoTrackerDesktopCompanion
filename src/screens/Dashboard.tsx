import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import {
  AlertTriangle,
  BookOpenText,
  Clock,
  Film,
  Inbox,
  MonitorPlay,
  Pause,
  RefreshCw,
  Type,
} from 'lucide-react';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/Empty';
import { Page, SectionTitle, StatTile } from '../components/ui/Page';
import { getMediaTypeClasses } from '../constants/mediaColors';
import { formatClock, formatDuration, numberWithCommas, readingSpeed } from '../lib/format';
import { ipc } from '../lib/ipc';
import { useAppStore } from '../store/useAppStore';
import type { AnimeSession, VnSession } from '../lib/types';
import { MediaSearchModal } from '../components/MediaSearchModal';

export default function Dashboard() {
  const tracker = useAppStore((s) => s.tracker);
  const settings = useAppStore((s) => s.settings);
  const online = useAppStore((s) => s.online);
  const refreshConnection = useAppStore((s) => s.refreshConnection);
  const [syncing, setSyncing] = useState(false);

  async function syncNow() {
    setSyncing(true);
    try {
      await ipc.flushOutbox();
      await refreshConnection();
    } finally {
      setSyncing(false);
    }
  }

  const anime = tracker?.anime ?? null;
  const vn = tracker?.vn ?? null;
  const idle = tracker?.idle_seconds ?? 0;

  return (
    <Page
      title="Actividad"
      description={
        tracker?.platform_supported
          ? 'Detección en marcha. Deja el companion abierto mientras ves o lees.'
          : 'Detección no disponible en esta plataforma todavía.'
      }
      actions={
        <Button appearance="outline" size="sm" loading={syncing} onClick={() => void syncNow()}>
          <RefreshCw className="w-4 h-4" />
          Sincronizar
        </Button>
      }
    >
      {online === false ? (
        <div className="alert alert-warning">
          <AlertTriangle className="w-5 h-5" />
          <span className="text-sm">
            Sin conexión con el servidor. Todo se guarda en local y se enviará al reconectar
            {tracker?.queued_count ? ` (${tracker.queued_count} en cola)` : ''}.
          </span>
        </div>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-2">
        {anime ? <AnimeCard session={anime} /> : <IdleCard kind="anime" />}
        {vn ? <VnCard session={vn} idleSeconds={idle} /> : <IdleCard kind="vn" />}
      </div>

      <SectionTitle>Resumen</SectionTitle>
      <div className="grid gap-3 grid-cols-2 lg:grid-cols-4">
        <StatTile
          label="Pendientes"
          value={tracker?.pending_count ?? 0}
          hint="esperan confirmación"
          accent="text-warning"
        />
        <StatTile
          label="En cola"
          value={tracker?.queued_count ?? 0}
          hint="sin enviar al servidor"
          accent="text-info"
        />
        <StatTile
          label="Inactividad"
          value={formatDuration(idle)}
          hint={`AFK a los ${settings?.afk_timeout_secs ?? 0}s`}
          accent="text-base-content"
        />
        <StatTile
          label="Ventana activa"
          value={<span className="text-sm font-medium">{tracker?.active_process ?? '—'}</span>}
          hint={tracker?.active_window ?? undefined}
          accent="text-base-content"
        />
      </div>

      {(tracker?.pending_count ?? 0) > 0 ? (
        <Link to="/queue" className="btn btn-warning btn-block">
          <Inbox className="w-4 h-4" />
          Revisar {tracker?.pending_count} registro(s) pendiente(s)
        </Link>
      ) : null}
    </Page>
  );
}

function IdleCard({ kind }: { kind: 'anime' | 'vn' }) {
  const isAnime = kind === 'anime';
  return (
    <div className="surface overflow-hidden">
      <div className={`h-1 w-full ${getMediaTypeClasses(isAnime ? 'anime' : 'vn').accentColor}`} />
      <EmptyState
        icon={isAnime ? MonitorPlay : BookOpenText}
        title={isAnime ? 'Ningún reproductor detectado' : 'Ninguna novela visual activa'}
        description={
          isAnime
            ? 'Abre mpv, MPC-HC, VLC o PotPlayer y el episodio se reconocerá solo.'
            : 'Registra el ejecutable de tus VNs en la biblioteca para que se detecten al abrirlas.'
        }
        action={
          !isAnime ? (
            <Link to="/library" className="btn btn-primary btn-sm">
              Añadir una novela
            </Link>
          ) : undefined
        }
      />
    </div>
  );
}

function AnimeCard({ session }: { session: AnimeSession }) {
  const type = getMediaTypeClasses('anime');
  const [matching, setMatching] = useState(false);
  const progress = session.threshold_secs
    ? Math.min(100, Math.round((session.elapsed_secs / session.threshold_secs) * 100))
    : 100;

  const title = session.media?.title ?? session.parsed.title;

  return (
    <article className={`surface overflow-hidden border ${type.borderColor}`}>
      <div className={`h-1 w-full ${type.accentColor}`} />
      <div className="p-4 space-y-3">
        <header className="flex items-start gap-3">
          {session.media?.image ? (
            <img
              src={session.media.image}
              alt=""
              className="w-14 h-20 object-cover rounded-field shrink-0"
            />
          ) : (
            <div className="w-14 h-20 rounded-field bg-base-200 flex items-center justify-center shrink-0">
              <Film className="w-5 h-5 text-base-content/40" />
            </div>
          )}
          <div className="min-w-0 flex-1">
            <div className={`badge badge-outline ${type.color} gap-1`}>
              <MonitorPlay className="w-3 h-3" />
              <span className="text-xs font-medium">{session.player}</span>
            </div>
            <h3 className="font-bold text-base leading-tight mt-1.5 truncate">{title}</h3>
            <p className="text-xs text-base-content/60 truncate">{session.file_name}</p>
          </div>
        </header>

        <div className="flex flex-wrap gap-2">
          {session.parsed.episode !== null ? (
            <span className={`badge badge-soft gap-1 ${type.bgColor} ${type.color}`}>
              Episodio {session.parsed.episode}
              {session.parsed.episode_end ? `–${session.parsed.episode_end}` : ''}
            </span>
          ) : null}
          {session.parsed.season ? (
            <span className="badge badge-soft gap-1">Temporada {session.parsed.season}</span>
          ) : null}
          <span className="badge badge-ghost gap-1">
            <Clock className="w-3 h-3" />
            {formatClock(session.elapsed_secs)}
          </span>
        </div>

        {!session.resolved ? (
          <p className="text-xs text-base-content/60">Buscando en NihongoTracker…</p>
        ) : null}

        {session.unmatched ? (
          <div className="alert alert-warning py-2">
            <AlertTriangle className="w-4 h-4" />
            <span className="text-xs">Sin coincidencia en NihongoTracker.</span>
            <Button size="xs" appearance="outline" onClick={() => setMatching(true)}>
              Vincular
            </Button>
          </div>
        ) : null}

        <div>
          <div className="flex justify-between text-xs text-base-content/60 mb-1">
            <span>{session.logged ? 'Registrado' : 'Progreso hasta registrar'}</span>
            <span>{progress}%</span>
          </div>
          <progress
            className={`progress w-full ${session.logged ? 'progress-success' : 'progress-info'}`}
            value={progress}
            max={100}
          />
        </div>
      </div>

      <MediaSearchModal
        open={matching}
        onClose={() => setMatching(false)}
        mediaType="anime"
        initialQuery={session.parsed.title}
        onPick={async (hit) => {
          await ipc.setTitleMapping(session.parsed.title, {
            content_id: hit.contentId,
            title: hit.title.contentTitleRomaji || hit.title.contentTitleNative,
            media_type: hit.type ?? 'anime',
            image: hit.contentImage ?? hit.coverImage,
            episode_duration: hit.episodeDuration ?? null,
            manual: true,
          });
          setMatching(false);
        }}
      />
    </article>
  );
}

function VnCard({ session, idleSeconds }: { session: VnSession; idleSeconds: number }) {
  const type = getMediaTypeClasses(session.media_type);
  const [, force] = useState(0);

  // The Rust side pushes a snapshot every second, but re-render anyway so the
  // clock never looks frozen if a tick is missed.
  useEffect(() => {
    const id = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);

  const speed = readingSpeed(session.session_chars, session.elapsed_secs);

  return (
    <article className={`surface overflow-hidden border ${type.borderColor}`}>
      <div className={`h-1 w-full ${type.accentColor}`} />
      <div className="p-4 space-y-3">
        <header className="flex items-start gap-3">
          {session.image ? (
            <img
              src={session.image}
              alt=""
              className="w-14 h-20 object-cover rounded-field shrink-0"
            />
          ) : (
            <div className="w-14 h-20 rounded-field bg-base-200 flex items-center justify-center shrink-0">
              <BookOpenText className="w-5 h-5 text-base-content/40" />
            </div>
          )}
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <span
                className={`w-2 h-2 rounded-full ${
                  session.counting ? 'bg-success recording-dot' : 'bg-base-content/30'
                }`}
              />
              <span className="text-xs font-medium text-base-content/70">
                {session.counting ? 'Contando tiempo' : 'En pausa automática'}
              </span>
            </div>
            <h3 className="font-bold text-base leading-tight mt-1 truncate">
              {session.media_title}
            </h3>
            <p className="text-xs text-base-content/60 truncate">{session.exe}</p>
          </div>
        </header>

        <div className="grid grid-cols-3 gap-2 text-center">
          <div className="rounded-field bg-base-200/60 py-2">
            <div className="text-[10px] uppercase tracking-wide text-base-content/60">Tiempo</div>
            <div className="font-bold tabular-nums">{formatClock(session.elapsed_secs)}</div>
          </div>
          <div className="rounded-field bg-base-200/60 py-2">
            <div className="text-[10px] uppercase tracking-wide text-base-content/60">Caracteres</div>
            <div className="font-bold tabular-nums">{numberWithCommas(session.session_chars)}</div>
          </div>
          <div className="rounded-field bg-base-200/60 py-2">
            <div className="text-[10px] uppercase tracking-wide text-base-content/60">Car./hora</div>
            <div className="font-bold tabular-nums">{numberWithCommas(speed)}</div>
          </div>
        </div>

        {!session.counting ? (
          <p className="text-xs text-base-content/60 flex items-center gap-1.5">
            <Pause className="w-3 h-3" />
            {idleSeconds > 0
              ? `Sin actividad desde hace ${formatDuration(idleSeconds)}.`
              : 'La ventana no está en primer plano.'}
          </p>
        ) : null}

        <Link to="/texthooker" className="btn btn-primary btn-sm w-full">
          <Type className="w-4 h-4" />
          Abrir texthooker
        </Link>
      </div>
    </article>
  );
}
