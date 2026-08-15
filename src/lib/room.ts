import { io, type Socket } from 'socket.io-client';

/**
 * Shared texthooker rooms, same protocol as the web client
 * (`Backend/src/index.ts` socket handlers).
 *
 * The socket handshake authenticates with the `jwt` cookie, which a native app
 * does not have — but the server treats that as optional and lets `join_room`
 * carry the username instead, so rooms work here with the API-key identity.
 */

/** Wire format for a line inside a room: `japaneseCount`, not `charsCount`. */
export interface RoomLine {
  id: string;
  text: string;
  japaneseCount: number;
  createdAt: string;
}

export interface RoomMember {
  id: string;
  role: 'host' | 'guest';
  username?: string;
  userId?: string;
}

export type RoomMode = 'local' | 'host' | 'guest';

export interface RoomHandlers {
  onJoined: (payload: { role: 'host' | 'guest'; roomId: string }) => void;
  onCreated: (payload: { roomId: string; hostToken: string }) => void;
  onHistory: (lines: RoomLine[]) => void;
  onLine: (line: RoomLine) => void;
  onLinesDeleted: (lineIds: string[]) => void;
  onLinesRestored: (lines: RoomLine[]) => void;
  onMembers: (members: RoomMember[]) => void;
  onError: (message: string) => void;
  onStatus: (connected: boolean) => void;
}

export interface RoomOptions {
  baseUrl: string;
  roomId: string;
  role: 'host' | 'guest';
  username?: string | null;
  hostToken?: string | null;
}

export class RoomConnection {
  private socket: Socket | null = null;

  constructor(
    private readonly options: RoomOptions,
    private readonly handlers: RoomHandlers
  ) {}

  connect() {
    const { baseUrl, roomId, role, username, hostToken } = this.options;
    const socket = io(baseUrl, { withCredentials: true, transports: ['websocket', 'polling'] });
    this.socket = socket;

    socket.on('connect', () => {
      this.handlers.onStatus(true);
      socket.emit('join_room', {
        roomId,
        role,
        hostToken: hostToken ?? undefined,
        username: username ?? undefined,
      });
    });
    socket.on('disconnect', () => this.handlers.onStatus(false));
    socket.on('connect_error', (error: Error) => this.handlers.onError(error.message));

    socket.on('room_created', this.handlers.onCreated);
    socket.on('room_joined', this.handlers.onJoined);
    socket.on('load_history', this.handlers.onHistory);
    socket.on('receive_line', this.handlers.onLine);
    socket.on('lines_deleted', (payload: { lineIds: string[] }) =>
      this.handlers.onLinesDeleted(payload.lineIds)
    );
    socket.on('lines_restored', (payload: { lines: RoomLine[] }) =>
      this.handlers.onLinesRestored(payload.lines)
    );
    socket.on('room_users_update', this.handlers.onMembers);
    socket.on('error_message', this.handlers.onError);
  }

  sendLine(line: RoomLine) {
    this.socket?.emit('send_line', { roomId: this.options.roomId, lineData: line });
  }

  deleteLines(lineIds: string[]) {
    this.socket?.emit('delete_lines', { roomId: this.options.roomId, lineIds });
  }

  restoreLines(lines: RoomLine[]) {
    this.socket?.emit('restore_lines', { roomId: this.options.roomId, lines });
  }

  get id(): string | undefined {
    return this.socket?.id;
  }

  get connected(): boolean {
    return this.socket?.connected ?? false;
  }

  disconnect() {
    this.socket?.disconnect();
    this.socket = null;
    this.handlers.onStatus(false);
  }
}

/** Rooms are 24h server-side; a short readable id is enough. */
export function randomRoomId(): string {
  return Math.random().toString(36).slice(2, 8);
}
