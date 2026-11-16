import { shellFailure } from "./shellFailure";

/** Why an address on the remote daemon machine's own loopback could not be opened through a forwarded port, as the
 * shell says. `failed` carries what the shell reported as `message`, which cannot be localized. */
export class LinkForwardError extends Error {
  readonly kind: "nothing_listening" | "forwarding_refused" | "disconnected" | "failed";
  /** The port on the remote machine. */
  readonly port: number;
  constructor(kind: LinkForwardError["kind"], port: number, detail = "") {
    super(detail);
    this.kind = kind;
    this.port = port;
  }
}

/** The address a failed browser open was made at, when that is a forwarded one: the one to show for opening by hand,
 * since the address as written leads to this computer. */
export class ForwardedLinkOpenError extends Error {
  readonly address: string;
  constructor(address: string, cause: unknown) {
    super("", { cause });
    this.address = address;
  }
}

const KINDS: readonly LinkForwardError["kind"][] = ["nothing_listening", "forwarding_refused", "disconnected", "failed"];

/** What a rejection of the shell's forwarding command is: a `LinkForwardError` where it says one of its kinds and the
 * port, else `undefined`. */
export function linkForwardError(failure: unknown): LinkForwardError | undefined {
  const { kind, port, detail } = shellFailure(failure);
  const known = KINDS.find((candidate) => candidate === kind);
  return known && port !== undefined ? new LinkForwardError(known, port, detail) : undefined;
}
