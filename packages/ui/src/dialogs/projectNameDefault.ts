import type { ProjectSource } from "../protocol";

/** Mirrors the daemon's lexical directory naming and git remote basename rules, for a hint only.
 * Home-relative paths need the daemon host's home; no local filesystem or symlink is consulted. */
export function projectNameDefault(source: ProjectSource, path: string, remoteUrl: string, home: string | null): string | undefined {
  if (source === "parent") return undefined;
  if (source === "git") {
    const last = remoteUrl.trim().replace(/\/+$/, "").split(/[/:]/).pop() ?? "";
    return last.replace(/\.git$/, "") || undefined;
  }
  let full = path.trim();
  if (full === "~" || full.startsWith("~/")) {
    const rest = full === "~" ? "" : full.slice(2);
    // Rust PathBuf.join replaces the home when the remainder is itself absolute (~//...).
    if (rest.startsWith("/")) full = rest;
    else {
      if (!home) return undefined;
      full = `${home}/${rest}`;
    }
  }
  if (!full.startsWith("/")) return undefined;
  const components: string[] = [];
  for (const component of full.split("/")) {
    if (component === "..") components.pop();
    else if (component && component !== ".") components.push(component);
  }
  return components.at(-1) ?? "/";
}
