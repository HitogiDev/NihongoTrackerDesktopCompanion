import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { events, ipc } from '../../lib/ipc';
import { RoomConnection, type RoomLine, type RoomMember } from '../../lib/room';
import type { Settings, TextLine } from '../../lib/types';

export type RoomMode = 'local' | 'host' | 'guest';

/** The shared room (Socket.IO from the webview, same protocol as the web). */
export function useRoom({
  settings,
  contentId,
  reload,
}: {
  settings: Settings | null;
  contentId: string;
  reload: () => Promise<void>;
}) {
  const [roomConnected, setRoomConnected] = useState(false);
  const [roomMembers, setRoomMembers] = useState<RoomMember[]>([]);
  const [roomError, setRoomError] = useState<string | null>(null);
  const [inviteCopied, setInviteCopied] = useState(false);
  const roomRef = useRef<RoomConnection | null>(null);

  const roomMode = (settings?.texthooker_room_mode ?? 'local') as RoomMode;
  const roomId = settings?.texthooker_room_id ?? '';
  const inviteLink =
    roomId && settings?.api_base_url
      ? `${settings.api_base_url.replace(/\/+$/, '')}/texthooker/session?mode=guest&roomId=${roomId}`
      : '';

  // A line produced locally is mirrored to the room when hosting.
  useEffect(() => {
    const unlisten = listen<{ line: TextLine }>(events.hookerLine, (event) => {
      const room = roomRef.current;
      if (!room?.connected || roomMode !== 'host') return;
      if (event.payload.line.content_id !== contentId) return;
      room.sendLine({
        id: event.payload.line.id,
        text: event.payload.line.text,
        japaneseCount: event.payload.line.chars,
        createdAt: event.payload.line.created_at,
      });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [contentId, roomMode]);

  const disconnectRoom = useCallback(() => {
    roomRef.current?.disconnect();
    roomRef.current = null;
    setRoomConnected(false);
    setRoomMembers([]);
  }, []);

  const connectRoom = useCallback(() => {
    if (!settings || roomMode === 'local' || !roomId.trim()) return;
    disconnectRoom();
    setRoomError(null);

    const room = new RoomConnection(
      {
        baseUrl: settings.api_base_url,
        roomId: roomId.trim(),
        role: roomMode,
        username: settings.username,
        // Same key the web client uses, so hosting the same room from either
        // client reclaims it instead of being rejected.
        hostToken: localStorage.getItem(`hostToken_${roomId.trim()}`),
      },
      {
        onStatus: setRoomConnected,
        onCreated: ({ roomId: id, hostToken }) =>
          localStorage.setItem(`hostToken_${id}`, hostToken),
        onJoined: () => setRoomError(null),
        onHistory: (history) => {
          void Promise.all(history.map((line) => ipc.hookerIngest(line.text))).then(reload);
        },
        onLine: (line: RoomLine) => {
          void ipc.hookerIngest(line.text);
        },
        onLinesDeleted: () => void reload(),
        onLinesRestored: () => void reload(),
        onMembers: setRoomMembers,
        onError: setRoomError,
      }
    );
    room.connect();
    roomRef.current = room;
  }, [settings, roomMode, roomId, disconnectRoom, reload]);

  // Leave the room when the screen goes away.
  useEffect(() => () => disconnectRoom(), [disconnectRoom]);

  const copyInvite = useCallback(() => {
    void navigator.clipboard.writeText(inviteLink).then(() => {
      setInviteCopied(true);
      setTimeout(() => setInviteCopied(false), 2000);
    });
  }, [inviteLink]);

  return {
    roomMode,
    roomId,
    inviteLink,
    roomConnected,
    roomMembers,
    roomError,
    inviteCopied,
    copyInvite,
    connectRoom,
    disconnectRoom,
  };
}
