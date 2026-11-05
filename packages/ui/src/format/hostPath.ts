/** How the daemon host writes an absolute path (`snapshot.host.path_style`, the protocol's `HostPathStyle`). Absolute paths travel in the host's
 * own spelling, so what a client does with one (splitting, shortening, telling it is absolute) depends on this and
 * never on the client's own platform. Project-relative wire paths are `/`-separated whatever the style and do not
 * come through here (see `wirePath`). */
export type PathStyle = "unix" | "windows";

/** The facts about the host that showing its paths needs: its home directory (null while unknown) and its path style. */
export interface HostPaths {
  home?: string | null;
  style: PathStyle;
}

/** The style before the first snapshot has said, so a client shows paths the way it did before hosts differed. */
export const DEFAULT_PATH_STYLE: PathStyle = "unix";

/** The `HostPaths` of the store's state: its home directory and its host's path style, `unix` until the first snapshot. */
export function hostPathsOf(state: { homeDir?: string | null; host?: { path_style: PathStyle } | null }): HostPaths {
  return { home: state.homeDir, style: state.host?.path_style ?? DEFAULT_PATH_STYLE };
}

const WINDOWS_DRIVE_ROOT = /^[A-Za-z]:[\\/]/;
const WINDOWS_UNC_ROOT = /^[\\/]{2}[^\\/?.][^\\/]*[\\/]+[^\\/]+[\\/]?/;
const WINDOWS_VERBATIM = /^[\\/]{2}[?.][\\/]/;

/** How many leading characters of `path` are its root: `/` on a Unix host; `C:\` or `\\server\share\` on a Windows
 * one, either separator accepted, and the `\\?\` form too. 0 for a path with no root, a drive-relative `C:x` or a
 * rooted-on-the-current-drive `\x` included, neither of which names one place on its own. */
function rootLength(path: string, style: PathStyle): number {
  if (style === "unix") return path.startsWith("/") ? 1 : 0;
  const verbatim = WINDOWS_VERBATIM.exec(path)?.[0].length ?? 0;
  const rest = path.slice(verbatim);
  const unc = verbatim > 0 ? /^UNC[\\/]+[^\\/]+[\\/]+[^\\/]+[\\/]?/i.exec(rest) : WINDOWS_UNC_ROOT.exec(rest);
  if (unc) return verbatim + unc[0].length;
  const drive = WINDOWS_DRIVE_ROOT.exec(rest);
  return drive ? verbatim + drive[0].length : 0;
}

function separators(style: PathStyle): RegExp {
  return style === "unix" ? /\/+/ : /[\\/]+/;
}

/** Whether `path` names one place by itself, on a host that writes paths in `style`. Only the shape is judged. */
export function isAbsolute(path: string, style: PathStyle): boolean {
  return rootLength(path, style) > 0;
}

/** `path` as its root (empty for a relative one) and the names below it, empty names left out. A backslash separates
 * names only on a Windows host: on a Unix one it is part of a name. */
export function split(path: string, style: PathStyle): { root: string; names: string[] } {
  const length = rootLength(path, style);
  return { root: path.slice(0, length), names: path.slice(length).split(separators(style)).filter(Boolean) };
}

/** The last name of `path`, or its root when it has no name (`/`, `C:\`). */
export function basename(path: string, style: PathStyle): string {
  const { root, names } = split(path, style);
  return names.at(-1) ?? root;
}

/** `path` without the separators at its end, its root kept whole. */
export function trimTrailingSeparators(path: string, style: PathStyle): string {
  const length = rootLength(path, style);
  return path.slice(0, length) + path.slice(length).replace(style === "unix" ? /\/+$/ : /[\\/]+$/, "");
}

/** `base` with the project-relative wire path `relative` below it, written with the host's own separator: the
 * `/`-separated names of a wire path become `\` names on a Windows host. A root such as `/` or `C:\` takes no second
 * separator. */
export function join(base: string, relative: string, style: PathStyle): string {
  const separator = style === "windows" ? "\\" : "/";
  const head = trimTrailingSeparators(base, style);
  const names = relative.split("/").filter(Boolean);
  if (names.length === 0) return head;
  const closed = separators(style).test(head.slice(-1));
  return `${head}${closed ? "" : separator}${names.join(separator)}`;
}

/** The directory containing `path`, in the spelling it was given, or `undefined` when there is nothing above it: a root,
 * or a relative path of one name. */
export function parent(path: string, style: PathStyle): string | undefined {
  const length = rootLength(path, style);
  const trimmed = trimTrailingSeparators(path, style);
  if (trimmed.length === length) return undefined;
  const at = Math.max(trimmed.lastIndexOf("/"), style === "windows" ? trimmed.lastIndexOf("\\") : -1);
  if (at < 0) return undefined;
  return trimmed.slice(0, at < length ? length : at);
}

/** Whether `path` is `~` or starts with `~` and a separator, which the daemon expands to its home directory
 * (`~\` too on a Windows host). */
export function isHomeRelative(path: string, style: PathStyle): boolean {
  return path === "~" || path.startsWith("~/") || (style === "windows" && path.startsWith("~\\"));
}

/** `path` with a leading `~` replaced by `home`. */
export function expandHome(path: string, home: string, style: PathStyle): string {
  return path.replace(style === "unix" ? /^~(?=\/|$)/ : /^~(?=[\\/]|$)/, () => home);
}

/** Whether `path` names a place the daemon resolves on its own: absolute, or home-relative. Only the shape is judged,
 * not whether the directory exists. */
export function isAbsoluteOrHome(path: string, style: PathStyle): boolean {
  const trimmed = path.trim();
  return isAbsolute(trimmed, style) || isHomeRelative(trimmed, style);
}

/** The path as it is shown to a person. On a Unix host the home directory itself or a path below it is written with a
 * leading `~`: a sibling that merely shares the prefix (`/Users/devx` against `/Users/dev`) is a different directory
 * and stays as it is, a trailing `/` on `home` is tolerated, and a path comes back unchanged when `home` is unknown
 * or is the filesystem root (which would shorten every path). On a Windows host a path is always shown in full
 * (a `~` is not a thing there). Display only: what is sent to the daemon is never built from this. */
export function display(path: string, { home, style }: HostPaths): string {
  if (style === "windows") return path;
  const base = home?.replace(/\/+$/, "");
  if (!base) return path;
  if (path === base) return "~";
  if (path.startsWith(`${base}/`)) return `~${path.slice(base.length)}`;
  return path;
}

/** Whether `path`, as found in running text (a terminal's output), may name a file or folder worth asking the daemon
 * about: absolute, home-relative or a relative one with a directory in it, or, with a `:line` after it (`hasLine`), a
 * file name. Whether it exists is the daemon's to say. A Windows host takes everything a Unix one does, as a Unix-style
 * path is how some tools spell one there, and adds `C:\…`, `\\server\share\…`, `~\…` and names with a `\` in them. */
export function isPathLike(path: string, hasLine: boolean, style: PathStyle): boolean {
  if (path.startsWith("/")) return path.length > 1;
  if (path.startsWith("~/")) return path.length > 2;
  if (style === "windows") {
    const length = rootLength(path, style);
    if (length > 0) return path.length > length;
    if (path.startsWith("~\\")) return path.length > 2;
  }
  if (!/[\p{L}\p{N}_]/u.test(path)) return false;
  return path.includes("/") || (style === "windows" && path.includes("\\")) || (hasLine && /[^.]\.[^.]/.test(path));
}
