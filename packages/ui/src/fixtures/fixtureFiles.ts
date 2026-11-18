// What the fixture daemon answers for files and folders: the files of a project, the folders the add-project dialog lists
// and the paths a terminal shows as links.
import type { BrowseEntry, DirEntry, FileContent, PathLinkKind, PathLinkView } from "../protocol";
import { refuseUnfillable } from "./banner";
import { SAMPLE_FILES, type FixtureFile, type FixtureFiles } from "./sampleFiles";
import { HOME, neverReplied, normalizedPath, type FixtureContext, type Handlers, type Handler } from "./fixtureContext";

/** The names the real daemon's file name search skips at every depth, with everything beneath them:
 * Git's metadata and the files an operating system leaves in a folder, matched ignoring ASCII case
 * (stored folded), and `._` starts AppleDouble. */
const LEFT_OUT_NAMES = new Set([
  ".git",
  ".ds_store",
  ".appledouble",
  ".spotlight-v100",
  ".trashes",
  ".fseventsd",
  ".temporaryitems",
  "thumbs.db",
  "desktop.ini",
]);

/** Only A to Z are folded, as the daemon's `eq_ignore_ascii_case` does; `toLowerCase` would also fold
 * characters like the Kelvin sign into a letter. */
function isLeftOut(name: string): boolean {
  return LEFT_OUT_NAMES.has(name.replace(/[A-Z]/g, (letter) => letter.toLowerCase())) || name.startsWith("._");
}

/** The temporary folders a path link can lie in without being the project's: `open_path_link` says `root: "temporary"` of a
 * path gone from one. */
const TEMPORARY_ROOTS = ["/tmp", "/private/tmp", "/var/folders", "/private/var/folders"];
const isInTemporaryRoot = (path: string) => TEMPORARY_ROOTS.some((root) => normalizedPath(path).startsWith(`${root}/`));

/** The most a whole read delivers (`MAX_FILE_BYTES` of the daemon's browse budgets); a file past it is refused with
 * `limit_exceeded`, which the viewer shows as a stand-in. */
const FILE_BYTES = 4 * 1024 * 1024;

/** The most one `read_file_range` part delivers (`MAX_RANGE_BYTES`). */
const RANGE_BYTES = 1024 * 1024;

/** The bytes a fixture file holds, for a read in parts: its text, the bytes of its image, or zeros as many as a binary
 * of no known type reports. */
function bytesOfFile(file: FixtureFile, from: number, count: number): { size: number; bytes: Uint8Array } {
  if ("text" in file) {
    const bytes = new TextEncoder().encode(file.text);
    return { size: bytes.length, bytes: bytes.subarray(from, from + count) };
  }
  if ("image" in file) {
    const decoded = atob(file.image.data);
    const bytes = Uint8Array.from(decoded.slice(from, from + count), (char) => char.charCodeAt(0));
    return { size: decoded.length, bytes };
  }
  if ("binary" in file) return { size: file.binary.size, bytes: new Uint8Array(Math.max(0, Math.min(count, file.binary.size - from))) };
  return { size: 0, bytes: new Uint8Array(0) };
}

/** The base64 of `bytes`, as a part travels. */
function base64Of(bytes: Uint8Array): string {
  let binary = "";
  for (let at = 0; at < bytes.length; at += 0x8000) binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
  return btoa(binary);
}

/** A listing for a path the scenario gave none for: a few directories, some of them repositories. */
export function genericListing(path: string): { path: string; entries: DirEntry[] } {
  const resolved = path === "~" ? HOME : path;
  const base = resolved === "/" ? "" : resolved;
  const entries = ["code", "Documents", "Downloads", "work", "repos"].map((name, i) => ({
    name,
    path: `${base}/${name}`,
    is_git_repo: i % 2 === 0,
  }));
  return { path: resolved, entries };
}

/** Whether Git ignores `path`: the fixture marks it, or a folder above it, `ignored`. */
function isIgnored(files: FixtureFiles, path: string): boolean {
  const segments = path.split("/");
  return segments.some((_, i) => files[segments.slice(0, i + 1).join("/")]?.ignored === true);
}

/** The entries directly under `dir` among `files`, as `list_project_dir` sends them. */
function entriesUnder(files: FixtureFiles, dir: string): BrowseEntry[] {
  const prefix = dir === "" ? "" : `${dir}/`;
  return Object.entries(files)
    .filter(([path]) => path !== dir && path.startsWith(prefix) && !path.slice(prefix.length).includes("/"))
    .map(([path, file]): BrowseEntry => ({
      name: path.slice(prefix.length),
      kind: "dir" in file ? "directory" : "file",
      size: "text" in file ? new TextEncoder().encode(file.text).length : "image" in file ? Math.floor((file.image.data.length * 3) / 4) : "binary" in file ? file.binary.size : null,
      version: "dir" in file ? null : "1",
      target: null,
      ...(isIgnored(files, path) && { ignored: true }),
    }))
    .sort((a, b) => (a.name < b.name ? -1 : 1));
}

/** The requests that read a project's files, which `createFileReplies` answers (the Git ones are `fixtureGit.ts`'s, see
 * `GIT_REQUESTS`). The table below is typed by this list, so a request named here without a reply, or a reply for one
 * not named here, does not compile. */
export const FILE_REQUESTS = ["list_project_dir", "search_project_files", "read_project_file", "read_path_link", "read_file_range"] as const;
type FileRequest = (typeof FILE_REQUESTS)[number];

/** The replies about files and folders: a project's own, the folders the add-project dialog lists, and the paths the
 * terminal shows as links. */
export function createFileReplies({ scenario, store, fail, restored }: FixtureContext): Handlers {
  const projectFiles = (project: string) => restored.files.get(project) ?? scenario.filesByProject?.[project] ?? scenario.files ?? SAMPLE_FILES;
  /** The absolute spelling of a path the terminal showed: as given when absolute or home-relative, else under the asking project's folder. */
  const absoluteLinkPath = (project: string | undefined, path: string) => {
    const root = project === undefined ? undefined : store.getState().projects.get(project)?.path;
    return path.startsWith("/") || path.startsWith("~") || root === undefined ? path : `${root}/${path}`;
  };
  /** What the scenario's `pathLinks` holds for a path the terminal showed, resolved against the asking project's folder. */
  const pathLinkFound = (project: string | undefined, path: string) => {
    const absolute = absoluteLinkPath(project, path);
    return Object.entries(scenario.pathLinks ?? {}).find(([given]) => normalizedPath(given) === normalizedPath(absolute))?.[1];
  };
  /** Whether a path found is a file the system could launch rather than show, which the daemon answers `viewer_only` in
   * the probe and refuses to open: only the viewer shows it. */
  const isLaunchableFile = (found: NonNullable<ReturnType<typeof pathLinkFound>>) =>
    typeof found === "object" && "refused" in found && found.refused === "launchable" && found.kind === "file";
  /** Refuses a path the daemon will not take from a link, with the path as given, as the daemon does: one gone since the
   * output was drawn, one that lies outside every root now, one found but refused. Of the `launchable` refusals only a
   * launchable file is let through to a reveal and a read, as the daemon refuses only opening it (a launchable folder is
   * never found, and is refused by every request). */
  const refuseUnreachableLink = (body: { project?: string; path: string }, found: ReturnType<typeof pathLinkFound>, action: "open" | "reveal" | "read"): void => {
    const done = action === "open" ? "opened" : action === "reveal" ? "shown" : "read";
    if (typeof found === "object" && "gone" in found) {
      // `root` says where the path was: a temporary folder has nowhere to be found again, the project has its files.
      const root = isInTemporaryRoot(absoluteLinkPath(body.project, body.path)) ? "temporary" : "project";
      fail({ code: "file_not_found", params: { path: body.path, root }, message: `\`${body.path}\` does not exist` });
    }
    if (typeof found === "object" && "outside" in found) {
      fail({ code: "outside_scope", params: { path: body.path, action: "open" }, message: `${body.path} leads outside the project, so it is not ${done}` });
    }
    if (typeof found === "object" && "refused" in found && (action === "open" || found.refused !== "launchable" || !isLaunchableFile(found))) {
      // Only an `unreadable` cause carries the system's text, as the daemon sends `detail` for no other.
      const detail = found.refused === "unreadable" ? found.detail : undefined;
      fail({
        code: "path_link_refused",
        params: { path: body.path, cause: found.refused, ...(detail !== undefined && { detail }) },
        message: `${body.path} is not ${done} from a link: ${found.refused}`,
      });
    }
  };
  /** A browse request's path, or the daemon error it is answered with, worded as the store words one. */
  const browsed = (files: FixtureFiles, path: string): FixtureFile => {
    const file = files[path];
    if (file?.error) return fail(file.error);
    if (!file) return fail({ code: "file_not_found", params: { path }, message: `${path} was not found` });
    return file;
  };

  /** The file a project's request reads, whole: refused with `limit_exceeded` past what a read delivers, answered with
   * the size and, for a text or an image, the content. */
  const wholeFile = (file: FixtureFile): FileContent => {
    const { size } = bytesOfFile(file, 0, 0);
    if (size > FILE_BYTES) {
      fail({ code: "limit_exceeded", params: { limit: "file_bytes", max: String(FILE_BYTES), size: String(size) }, message: `the file is larger than the ${FILE_BYTES}-byte limit` });
    }
    return "text" in file
      ? { size, kind: "text", media_type: null, text: file.text, data: null }
      : "image" in file
        ? { size, kind: "binary", media_type: file.image.mediaType, text: null, data: file.image.data }
        : { size, kind: "binary", media_type: null, text: null, data: "" };
  };
  /** The file a path in a terminal's output names, as `read_path_link` and `read_file_range` resolve it again: refused as
   * the daemon refuses a path it will not take, read from the project's files when the path found has a `view`, else
   * from the `content` the scenario gives it. */
  const linkedFile = (body: { project?: string; path: string }): FixtureFile => {
    const found = pathLinkFound(body.project, body.path);
    refuseUnreachableLink(body, found, "read");
    if (found === undefined) {
      const root = isInTemporaryRoot(absoluteLinkPath(body.project, body.path)) ? "temporary" : "project";
      return fail({ code: "file_not_found", params: { path: body.path, root }, message: `\`${body.path}\` does not exist` });
    }
    if (found === "directory" || (typeof found === "object" && "kind" in found && found.kind === "directory")) {
      return fail({ code: "unsupported_file_type", params: { path: body.path, file_type: "directory" }, message: `${body.path} is a directory, which cannot be read here` });
    }
    const view = typeof found === "object" && "view" in found ? found.view : undefined;
    if (view) {
      if (body.project === undefined) return refuseUnfillable(`${body.path} has a view in a project, and the request names none`);
      return browsed(view.worktree === undefined ? projectFiles(body.project) : scenario.worktreeFiles?.[view.worktree] ?? {}, view.path);
    }
    const content = typeof found === "object" && "content" in found ? found.content : undefined;
    if (!content) return refuseUnfillable(`${body.path} is a file the viewer reads, and the scenario's pathLinks gives it no content`);
    if (content.error) return fail(content.error);
    return content;
  };

  const browse: { [K in FileRequest]: Handler<K> } = {
    list_project_dir: (body) => {
      const files = projectFiles(body.project);
      const dir = browsed(files, body.path);
      if ("dir" in dir && dir.pending) return neverReplied();
      const complete = "dir" in dir ? dir.complete !== false : true;
      return { type: "project_dir", id: null, project: body.project, worktree: null, path: body.path, root_id: "fixture", entries: entriesUnder(files, body.path), complete };
    },
    search_project_files: (body) => {
      if (scenario.searchError) return fail(scenario.searchError);
      const query = body.query.trim().toLowerCase();
      const files = projectFiles(body.project);
      const paths = Object.entries(files)
        .filter(([path, file]) => !("dir" in file) && !path.split("/").some(isLeftOut) && path.slice(path.lastIndexOf("/") + 1).toLowerCase().includes(query))
        .map(([path]) => path)
        .filter((path) => body.include_ignored === true || !isIgnored(files, path))
        .sort();
      const ignored = body.include_ignored === true ? paths.filter((path) => isIgnored(files, path)) : [];
      // A search that stopped at its limits or met a folder it could not read is not whole, as a listing cut short or failed is not.
      const complete = !Object.values(files).some((file) => "dir" in file && (file.complete === false || file.error !== undefined));
      return { type: "project_file_matches", id: null, project: body.project, query: body.query, paths, ...(ignored.length > 0 && { ignored }), complete };
    },
    read_project_file: (body) => {
      const files = body.worktree === undefined ? projectFiles(body.project) : scenario.worktreeFiles?.[body.worktree] ?? {};
      const file = browsed(files, body.path);
      const source = { kind: "live" as const, root_id: "fixture", version: "1" };
      return { type: "project_file", id: null, project: body.project, worktree: body.worktree ?? null, path: body.path, source, file: wholeFile(file) };
    },
    read_path_link: (body) => ({ type: "path_link_file", id: null, path: body.path, version: "1", file: wholeFile(linkedFile(body)) }),
    read_file_range: (body) => {
      const file =
        body.file.kind === "project"
          ? browsed(body.file.worktree === undefined ? projectFiles(body.file.project) : scenario.worktreeFiles?.[body.file.worktree] ?? {}, body.file.path)
          : linkedFile(body.file);
      if (body.version !== undefined && body.version !== "1") {
        return fail({ code: "source_changed", params: { path: body.file.path }, message: `${body.file.path} changed while it was being read` });
      }
      // A copy held part-way: the daemon answers this many parts of 1 MiB, and the next never comes.
      if ("binary" in file && file.binary.answeredParts !== undefined && body.offset >= file.binary.answeredParts * RANGE_BYTES) return neverReplied();
      const { size, bytes } = bytesOfFile(file, body.offset, Math.min(body.length, RANGE_BYTES));
      return { type: "file_range", id: null, offset: body.offset, size, version: "1", data: base64Of(bytes) };
    },
  };

  return {
    ...browse,
    open_path_link: (body) => {
      refuseUnreachableLink(body, pathLinkFound(body.project, body.path), "open");
      return { type: "ack", id: null };
    },
    // Nothing is launched, so a launchable file is revealed too; a launchable folder the daemon never reaches still is not.
    reveal_path_link: (body) => {
      refuseUnreachableLink(body, pathLinkFound(body.project, body.path), "reveal");
      return { type: "ack", id: null };
    },
    probe_path_links: (body) => {
      const links = body.paths.map((path): { path: string; kind: PathLinkKind; view?: PathLinkView; viewer_only?: boolean } => {
        const found = pathLinkFound(body.project, path);
        if (found === undefined) return { path, kind: "none" };
        if (typeof found === "string") return { path, kind: found };
        const kind = "kind" in found ? found.kind : "file";
        const view = "view" in found ? found.view : undefined;
        return { path, kind, ...(view && { view }), ...(isLaunchableFile(found) && { viewer_only: true }) };
      });
      return { type: "path_links", id: null, links };
    },
    list_dir: (body) => {
      // `~` is answered with the home directory it resolves to, as the real daemon does, so a
      // scenario names the listing it gives by that path.
      const path = body.path === "~" ? HOME : body.path;
      const listed = scenario.directories?.[path];
      if (listed && !Array.isArray(listed)) return fail(listed);
      return { type: "dir_listing", id: null, ...(listed ? { path, entries: listed } : genericListing(path)) };
    },
  };
}
