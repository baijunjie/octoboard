/** The path with the daemon host's home directory written as `~`, for showing to a person. Only
 * the home directory itself or a path below it is shortened: a sibling that merely shares the
 * prefix (`/Users/devx` against `/Users/dev`) is a different directory and stays as it is. A
 * trailing `/` on `home` is tolerated, and a path comes back unchanged when `home` is unknown or is
 * the filesystem root (which would shorten every path). Display only: what is sent to the daemon is
 * never built from this. */
export function abbreviateHome(path: string, home: string | null | undefined): string {
  const base = home?.replace(/\/+$/, "");
  if (!base) return path;
  if (path === base) return "~";
  if (path.startsWith(`${base}/`)) return `~${path.slice(base.length)}`;
  return path;
}
