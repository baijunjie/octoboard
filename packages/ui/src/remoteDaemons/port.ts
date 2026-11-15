/** The SSH port when nothing names one: a new entry's, and that of connection details that carry none. */
export const DEFAULT_SSH_PORT = 22;

/** The largest TCP port. */
const MAX_PORT = 65535;

/** The port `text` holds, surrounding spaces aside, or `undefined` when it is not a whole number from 1 to 65535 (0
 * would let the system choose). */
export function parsePort(text: string): number | undefined {
  const trimmed = text.trim();
  if (!/^\d{1,5}$/.test(trimmed)) return undefined;
  const port = Number(trimmed);
  return port >= 1 && port <= MAX_PORT ? port : undefined;
}
