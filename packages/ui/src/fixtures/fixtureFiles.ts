// What the fixture daemon answers for files and folders: the files of a project, the folders the add-project dialog lists
// and the paths a terminal shows as links.
import type { BrowseEntry, DirEntry, PathLinkKind, PathLinkView } from "../protocol";
import { SAMPLE_FILES, type FixtureFile, type FixtureFiles } from "./sampleFiles";
import { HOME, normalizedPath, type FixtureContext, type Handlers, type Handler } from "./fixtureContext";

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
export const FILE_REQUESTS = ["list_project_dir", "search_project_files", "read_project_file"] as const;
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
  /** A browse request's path, or the daemon error it is answered with, worded as the store words one. */
  const browsed = (files: FixtureFiles, path: string): FixtureFile => {
    const file = files[path];
    if (file?.error) return fail(file.error);
    if (!file) return fail({ code: "file_not_found", params: { path }, message: `${path} was not found` });
    return file;
  };

  const browse: { [K in FileRequest]: Handler<K> } = {
    list_project_dir: (body) => {
      const files = projectFiles(body.project);
      const dir = browsed(files, body.path);
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
      const content =
        "text" in file
          ? { size: new TextEncoder().encode(file.text).length, kind: "text" as const, media_type: null, text: file.text, data: null }
          : "image" in file
            ? { size: Math.floor((file.image.data.length * 3) / 4), kind: "binary" as const, media_type: file.image.mediaType, text: null, data: file.image.data }
            : { size: "binary" in file ? file.binary.size : 0, kind: "binary" as const, media_type: null, text: null, data: "" };
      return { type: "project_file", id: null, project: body.project, worktree: body.worktree ?? null, path: body.path, source, file: content };
    },
  };

  return {
    ...browse,
    open_path_link: (body) => {
      // A path found when the output was drawn that is gone now is refused as the daemon refuses it, with the path as given.
      const found = pathLinkFound(body.project, body.path);
      if (typeof found === "object" && "gone" in found) {
        // `root` says where the path was: a temporary folder has nowhere to be found again, the project has its files.
        const root = isInTemporaryRoot(absoluteLinkPath(body.project, body.path)) ? "temporary" : "project";
        return fail({ code: "file_not_found", params: { path: body.path, root }, message: `\`${body.path}\` does not exist` });
      }
      // One that lies outside every root now is refused as the daemon refuses an open from a link, with the path as given.
      if (typeof found === "object" && "outside" in found) {
        return fail({ code: "outside_scope", params: { path: body.path, action: "open" }, message: `${body.path} leads outside the project, so it is not opened` });
      }
      // One found but not opened from a link is refused as the daemon refuses it, with the path as given and why.
      if (typeof found === "object" && "refused" in found) {
        // Only an `unreadable` cause carries the system's text, as the daemon sends `detail` for no other.
        const detail = found.refused === "unreadable" ? found.detail : undefined;
        return fail({
          code: "path_link_refused",
          params: { path: body.path, cause: found.refused, ...(detail !== undefined && { detail }) },
          message: `${body.path} is not opened from a link: ${found.refused}`,
        });
      }
      return { type: "ack", id: null };
    },
    probe_path_links: (body) => {
      const links = body.paths.map((path): { path: string; kind: PathLinkKind; view?: PathLinkView; viewer_only?: boolean } => {
        const found = pathLinkFound(body.project, path);
        if (found === undefined) return { path, kind: "none" };
        if (typeof found === "string") return { path, kind: found };
        if ("gone" in found) return { path, kind: found.kind };
        if ("outside" in found) return { path, kind: found.kind, ...(found.view && { view: found.view }) };
        if ("refused" in found) return { path, kind: found.kind, ...(found.view && { view: found.view }) };
        if ("kind" in found) return { path, kind: found.kind, view: found.view };
        return { path, kind: "file", view: found.view, ...(found.viewerOnly && { viewer_only: true }) };
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
