import { DEFAULT_SSH_PORT, parsePort } from "./port";

/** The scheme of the connection details `octoboardd connection` prints. */
const SCHEME = "octoboard+ssh://";

/** What the connection details give a remote daemon entry. */
export interface ConnectionDetails {
  host: string;
  port: number;
  user: string;
  /** The host key fingerprint to pin, `SHA256:…`; absent when the remote machine could read no host key. */
  fingerprint?: string;
  octoboarddPath?: string;
  /** The name the remote machine gives itself, for the entry's name. */
  name?: string;
}

/** `text` percent-decoded, or undefined when it is not validly encoded. */
function decoded(text: string): string | undefined {
  try {
    return decodeURIComponent(text);
  } catch {
    return undefined;
  }
}

/**
 * The connection details in `text`, as `octoboardd connection` prints them —
 * `octoboard+ssh://<user>@<host>:<port>?fingerprint=…&octoboardd=…&name=…` — or undefined when `text` is not such
 * details. Spaces around the text are ignored; fields the application does not know are skipped, so that details of a
 * later `octoboardd` still fill what they can.
 */
export function parseConnectionDetails(text: string): ConnectionDetails | undefined {
  const trimmed = text.trim();
  if (trimmed.slice(0, SCHEME.length).toLowerCase() !== SCHEME) return undefined;
  const rest = trimmed.slice(SCHEME.length);
  const queryStart = rest.indexOf("?");
  const authority = (queryStart < 0 ? rest : rest.slice(0, queryStart)).replace(/\/$/, "");
  const query = queryStart < 0 ? "" : rest.slice(queryStart + 1);

  const at = authority.lastIndexOf("@");
  if (at < 0) return undefined;
  const user = decoded(authority.slice(0, at));
  const hostPort = authority.slice(at + 1);
  let hostText: string;
  let portText: string | undefined;
  if (hostPort.startsWith("[")) {
    const close = hostPort.indexOf("]");
    if (close < 0) return undefined;
    hostText = hostPort.slice(1, close);
    const after = hostPort.slice(close + 1);
    if (after && !after.startsWith(":")) return undefined;
    portText = after ? after.slice(1) : undefined;
  } else {
    const colon = hostPort.lastIndexOf(":");
    hostText = colon < 0 ? hostPort : hostPort.slice(0, colon);
    portText = colon < 0 ? undefined : hostPort.slice(colon + 1);
  }
  const host = decoded(hostText);
  if (!user || !host || /\s/.test(user) || /\s/.test(host)) return undefined;
  const port = portText === undefined ? DEFAULT_SSH_PORT : /\s/.test(portText) ? undefined : parsePort(portText);
  if (port === undefined) return undefined;

  const details: ConnectionDetails = { host, port, user };
  for (const field of query.split("&")) {
    const equals = field.indexOf("=");
    if (equals < 0) continue;
    const value = decoded(field.slice(equals + 1));
    if (value === undefined) return undefined;
    if (!value.trim()) continue;
    switch (field.slice(0, equals)) {
      case "fingerprint":
        details.fingerprint = value.trim();
        break;
      case "octoboardd":
        details.octoboarddPath = value;
        break;
      case "name":
        details.name = value.trim();
        break;
    }
  }
  return details;
}
