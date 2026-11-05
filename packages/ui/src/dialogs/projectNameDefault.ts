import { isAbsolute, isHomeRelative, type PathStyle, split } from "../format/hostPath";
import type { ProjectSource } from "../protocol";

/** Mirrors the daemon's lexical directory naming and git remote basename rules, for a hint only.
 * Home-relative paths need the daemon host's home; no local filesystem or symlink is consulted. */
export function projectNameDefault(source: ProjectSource, path: string, remoteUrl: string, home: string | null, style: PathStyle = "unix"): string | undefined {
  if (source === "parent") return undefined;
  if (source === "git") {
    const last = remoteUrl.trim().replace(/\/+$/, "").split(/[/:]/).pop() ?? "";
    return last.replace(/\.git$/, "") || undefined;
  }
  let full = path.trim();
  if (isHomeRelative(full, style)) {
    const rest = full === "~" ? "" : full.slice(2);
    // Rust PathBuf.join replaces the home when the remainder is itself absolute (~//... on Unix, ~\C:\... on Windows).
    if (isAbsolute(rest, style)) full = rest;
    else {
      if (!home) return undefined;
      full = `${home}${style === "unix" ? "/" : "\\"}${rest}`;
    }
  }
  if (!isAbsolute(full, style)) return undefined;
  const { root, names } = split(full, style);
  const components: string[] = [];
  for (const component of names) {
    if (component === "..") components.pop();
    else if (component !== ".") components.push(component);
  }
  return components.at(-1) ?? root;
}
