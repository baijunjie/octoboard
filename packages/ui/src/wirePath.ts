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
