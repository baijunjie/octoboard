/**
 * Display text for a wire path: a path, entry name or branch name in the daemon's canonical form,
 * where `%` travels as `%25` and every byte that is not part of valid UTF-8 as `%XX` (see "Wire
 * paths" in `apps/daemon/PROTOCOL.md`). The form is decoded to bytes and read as UTF-8 with
 * replacement characters, as the protocol asks. Display only: a request sends the wire form back
 * unchanged, never this text.
 */
export function displayWirePath(wire: string): string {
  if (!wire.includes("%")) return wire;
  const encoder = new TextEncoder();
  const bytes: number[] = [];
  for (let i = 0; i < wire.length; ) {
    if (wire[i] === "%" && /^[0-9A-Fa-f]{2}$/.test(wire.slice(i + 1, i + 3))) {
      bytes.push(parseInt(wire.slice(i + 1, i + 3), 16));
      i += 3;
      continue;
    }
    const codePoint = wire.codePointAt(i)!;
    const char = String.fromCodePoint(codePoint);
    for (const byte of encoder.encode(char)) bytes.push(byte);
    i += char.length;
  }
  return new TextDecoder("utf-8", { fatal: false }).decode(new Uint8Array(bytes));
}

/** The last component of a wire path, in display form; the project's own directory (the empty path)
 * has none. */
export function wireBaseName(wire: string): string {
  return displayWirePath(wire.slice(wire.lastIndexOf("/") + 1));
}

/** What to copy for the absolute wire path `path`, in display form: the path relative to the first
 * of `roots` that holds it (`.` for the root itself), or the absolute path when none does. A root
 * holds only paths below a whole component of it, so a sibling that merely shares its prefix
 * (`/work/app-2` beside `/work/app`) is not inside it; trailing slashes on either side are ignored. */
export function wireRelativeOrAbsolute(path: string, roots: readonly string[]): string {
  const trimmed = trimTrailingSlashes(path);
  for (const root of roots) {
    const base = trimTrailingSlashes(root);
    if (trimmed === base) return ".";
    const prefix = base.endsWith("/") ? base : `${base}/`;
    if (trimmed.startsWith(prefix)) return displayWirePath(trimmed.slice(prefix.length));
  }
  return displayWirePath(path);
}

function trimTrailingSlashes(wire: string): string {
  return wire.length > 1 ? wire.replace(/\/+$/, "") || "/" : wire;
}
