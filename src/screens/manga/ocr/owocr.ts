/** owocr's own default `websocket_port`. */
export const OWOCR_DEFAULT_PORT = 7331;

/** The command that starts owocr the way the app talks to it. */
export function owocrCommand(port: number): string {
  const portArg = port === OWOCR_DEFAULT_PORT ? '' : ` -wp ${port}`;
  return `owocr -r websocket -w websocket -of json${portArg}`;
}

/** A port typed in settings, or `null` if it is not one. */
export function parsePort(value: string): number | null {
  if (!/^\d{1,5}$/.test(value.trim())) return null;
  const port = Number(value);
  return port >= 1 && port <= 65535 ? port : null;
}
