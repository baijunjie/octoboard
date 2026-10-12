import { DaemonRequestError } from "../daemon-client";
import { daemonMessage } from "../daemonMessage";
import { currentLanguage } from "../i18n/language";
import type { BrowseEntry, ChangeSide, DirEntry, Event, FileContent, SideRead, SideRef } from "../protocol";
import { createStateStore, type Daemon, type ToastRequest } from "../store";
import type { Scenario } from "./scenario";
import { galleryFailure } from "./banner";
import { terminalFixtureUrl } from "./fakeTerminal";
import { SAMPLE_MODELS } from "./fixtures/agentModels";
import { text } from "./fixtures/builders";
import { SAMPLE_FILES, type FixtureError, type FixtureFile, type FixtureFiles } from "./fixtures/projectFiles";
import { SAMPLE_GIT } from "./fixtures/projectGit";

/** How long after the app subscribes to toasts the scenario's own are raised, for the stack to be
 * on screen to receive them. */
const TOAST_DELAY_MS = 300;

const HOME = "/Users/dev";

/** A listing for a path the scenario gave none for: a few directories, some of them repositories. */
function genericListing(path: string): { path: string; entries: DirEntry[] } {
  const resolved = path === "~" ? HOME : path;
  const base = resolved === "/" ? "" : resolved;
  const entries = ["code", "Documents", "Downloads", "work", "repos"].map((name, i) => ({
    name,
    path: `${base}/${name}`,
    is_git_repo: i % 2 === 0,
  }));
  return { path: resolved, entries };
}

/** How many lines a text holds: its newlines, and one more when the last line is unterminated. It
 * repeats `countLines` of `viewer/budgets.ts` on purpose: the fixture stands in for the daemon's
 * count and does not depend on a viewer render-budget helper. */
function lineCount(content: string): number {
  return content === "" ? 0 : content.split("\n").length - (content.endsWith("\n") ? 1 : 0);
}

/** A body's bytes with one character each, which `lineCount` then counts as the daemon counts the
 * bytes it read: a text body's newline characters are its newline bytes, and a binary one is
 * decoded so that an image counts the newlines in its own bytes, as the daemon's does. */
function bodyBytes(body: FileContent): string {
  return body.text ?? atob(body.data ?? "");
}

/**
 * Why a fixture cannot be answered as the real daemon would answer. Deliberately not a
 * `DaemonRequestError`: scenarios answer requests with a daemon error on purpose, and those are
 * legitimate UI states under test, so a gap in the gallery's own data must not be dressed as one.
 */
export class FixtureUnfillableError extends Error {
  constructor(reason: string) {
    super(`gallery fixture cannot answer faithfully — ${reason}`);
    this.name = "FixtureUnfillableError";
  }
}

/** Refuses a reply the fixture cannot fill as the real daemon would. It goes on the page as well as
 * to the rejected request, because the window swallows a failed read into the diff's own error line
 * and a scenario would otherwise look merely broken rather than under-filled. */
function refuseUnfillable(reason: string): never {
  const error = new FixtureUnfillableError(reason);
  galleryFailure(error.message);
  throw error;
}

/** The entries directly under `dir` among `files`, as `list_project_dir` sends them. */
function entriesUnder(files: FixtureFiles, dir: string): BrowseEntry[] {
  const prefix = dir === "" ? "" : `${dir}/`;
  return Object.entries(files)
    .filter(([path]) => path !== dir && path.startsWith(prefix) && !path.slice(prefix.length).includes("/"))
    .map(([path, file]): BrowseEntry => ({
      name: path.slice(prefix.length),
      kind: "dir" in file ? "directory" : "file",
      size: "text" in file ? new TextEncoder().encode(file.text).length : "image" in file ? Math.floor((file.image.data.length * 3) / 4) : null,
      version: "dir" in file ? null : "1",
      target: null,
    }))
    .sort((a, b) => (a.name < b.name ? -1 : 1));
}

/**
 * A `Daemon` with no connection behind it, for the gallery: the store holds the scenario's state as
 * it is, requests are answered from the scenario (`list_dir`, `list_pages`, the browse requests) or accepted and
 * ignored (a new console excepted, which is added), and the scenario's toasts are raised once the app is listening.
 */
export function createFixtureDaemon(scenario: Scenario): Daemon {
  const store = createStateStore(scenario.state);
  const toastListeners = new Set<(toast: ToastRequest) => void>();
  const emitToast = (toast: ToastRequest) => toastListeners.forEach((listener) => listener(toast));
  let toastsRaised = false;

  const raiseScenarioToasts = () => {
    if (toastsRaised) return;
    toastsRaised = true;
    for (const { code, params, message, ...toast } of scenario.toasts ?? []) {
      emitToast({
        ...toast,
        message: code ? daemonMessage(currentLanguage(), code, params ?? {}, message, store.getState()) : message,
      });
    }
  };

  const files = scenario.files ?? SAMPLE_FILES;
  const git = scenario.git === undefined ? SAMPLE_GIT : scenario.git;
  /** Whether a comparison has been made, after which the branch list tells of the branches moved. */
  let compared = false;
  const fail = ({ code, params, message }: FixtureError): never => {
    throw new DaemonRequestError(daemonMessage(currentLanguage(), code, params, message, store.getState()), code, params);
  };
  /**
   * A side of a fixture change as `read_project_change` reads it, with its body when it has one and
   * its line count where the daemon counts one (`apps/daemon/PROTOCOL.md`, "Changes and
   * comparisons"). Only a file side ever carries either: the daemon reads no content for a link or
   * a submodule. A carried body is counted from the body itself, whatever the patch holds;
   * otherwise only a side the daemon read an object for is counted, from `content`, the whole text
   * of that version. So `lines` stays null for a link or a submodule, for an absent or out-of-scope
   * side, for a side read from the worktree disk with no body, and for a side of a patch with no
   * hunk (`hunks` false) that carries no body, where no separator bar needs a count. Those are what
   * the real daemon leaves out, not gaps.
   *
   * Where the daemon would count and the fixture carries no `content` to count, the reply is
   * refused rather than sent with the field unfilled: a reply the daemon never sends is worse than
   * no reply, because it is believed. The gallery is where a UI state is looked at before it is
   * staged through a daemon, so a combination the app cannot produce reads there as the app's
   * defect. An unfilled count is a legal value of the reply, so nothing downstream — the type
   * check, the renderer, a scenario — can tell it from a real one.
   */
  const readSide = (at: string, side: ChangeSide, body: FileContent | undefined, content?: string, hunks = true): SideRead => {
    if (side.state === "absent") return side;
    if (side.state !== "present") return { state: "out_of_scope" };
    if (side.kind !== "file") return { ...side, file: null, lines: null };
    if (body) return { ...side, file: body, lines: lineCount(bodyBytes(body)) };
    if (!hunks || side.source.kind === "live") return { ...side, file: null, lines: null };
    if (content === undefined) {
      refuseUnfillable(`${at} (${side.path}): the daemon counts this side's lines, and the fixture carries no whole text to count`);
    }
    return { ...side, file: null, lines: lineCount(content) };
  };
  /** The daemon's refusal to read a side that is not a regular file, which a request for a change's
   * bodies meets before it reads anything. */
  const notAFile = (side: ChangeSide): FixtureError | undefined => {
    if (side.state !== "present" || side.kind === "file") return undefined;
    const params = { path: side.path, file_type: side.kind };
    return { code: "unsupported_file_type", params, message: `${side.path} is a ${side.kind}, which cannot be read here` };
  };
  /** The worktree a request is answered from: the one it names, else the project's own, as the
   * daemon resolves an omitted one. */
  const worktreeOf = (named: string | undefined) => named ?? git?.worktrees[0]?.id ?? "";
  /** Whether a fixture change's patch has a hunk, which a count of its sides is for. */
  const hasHunk = (found: { patch?: string }) => /^@@ /m.test(found.patch ?? "");
  const sameSide = (listed: ChangeSide, asked: SideRef) =>
    listed.state === asked.state && (listed.state !== "present" || (asked.state === "present" && asked.path === listed.path));
  /** A browse request's path, or the daemon error it is answered with, worded as the store words one. */
  const browsed = (path: string): FixtureFile => {
    const file = files[path];
    if (file?.error) {
      const { code, params, message } = file.error;
      throw new DaemonRequestError(daemonMessage(currentLanguage(), code, params, message, store.getState()), code, params);
    }
    if (!file) {
      const params = { path };
      throw new DaemonRequestError(daemonMessage(currentLanguage(), "file_not_found", params, `${path} was not found`, store.getState()), "file_not_found", params);
    }
    return file;
  };

  return {
    store,
    request: async (body): Promise<Event> => {
      switch (body.type) {
        case "list_project_dir": {
          const dir = browsed(body.path);
          const complete = "dir" in dir ? dir.complete !== false : true;
          return { type: "project_dir", project: body.project, worktree: null, path: body.path, root_id: "fixture", entries: entriesUnder(files, body.path), complete };
        }
        case "read_project_file": {
          const file = browsed(body.path);
          const source = { kind: "live" as const, root_id: "fixture", version: "1" };
          const content =
            "text" in file
              ? { size: new TextEncoder().encode(file.text).length, kind: "text" as const, media_type: null, text: file.text, data: null }
              : "image" in file
                ? { size: Math.floor((file.image.data.length * 3) / 4), kind: "binary" as const, media_type: file.image.mediaType, text: null, data: file.image.data }
                : { size: 0, kind: "binary" as const, media_type: null, text: null, data: "" };
          return { type: "project_file", project: body.project, worktree: null, path: body.path, source, file: content };
        }
        case "get_project_source": {
          const own = git?.worktrees[0];
          return {
            type: "project_source",
            source: {
              project: body.project,
              root: "/Users/dev/code/search-api",
              resolved_root: "/Users/dev/code/search-api",
              root_id: "fixture",
              git:
                git && !git.gitError && own
                  ? { repository: "repo", common_dir: "/Users/dev/code/search-api/.git", worktree: own.id, scope: "", worktrees: git.worktrees }
                  : null,
              git_error: git?.gitError ?? null,
            },
          };
        }
        case "list_project_changes": {
          const worktree = worktreeOf(body.worktree);
          const changes = git?.changes[worktree];
          if (!changes) return fail({ code: "worktree_unavailable", params: { worktree }, message: `worktree ${worktree} is gone` });
          if (!Array.isArray(changes)) return fail(changes);
          return {
            type: "project_changes",
            project: body.project,
            worktree: body.worktree ?? null,
            head: git?.worktrees.find((w) => w.id === worktree)?.head ?? null,
            changes: changes.map((change) => change.entry),
            complete: true,
          };
        }
        case "list_project_branches": {
          const moved = compared ? (git?.moved ?? {}) : {};
          const branches = (git?.branches ?? []).map((branch) => ({ ...branch, commit: moved[branch.name] ?? branch.commit }));
          return { type: "project_branches", project: body.project, branches, complete: true };
        }
        case "compare_project_branches": {
          const resolve = (name: string) => {
            const branch = git?.branches?.find((b) => b.name === name);
            if (!branch || git?.missing?.includes(name)) return fail({ code: "unknown_branch", params: { branch: name }, message: `there is no local branch ${name}` });
            return { branch: name, commit: branch.commit };
          };
          const left = resolve(body.left);
          const right = resolve(body.right);
          compared = true;
          const changes = left.commit === right.commit ? [] : (git?.comparisons?.[`${body.left}..${body.right}`] ?? []);
          return { type: "project_comparison", project: body.project, left, right, changes: changes.map((change) => change.entry), complete: true };
        }
        case "read_project_comparison_change": {
          const changes = git?.comparisons?.[`${body.left.branch}..${body.right.branch}`] ?? [];
          const found = changes.find(({ entry }) => entry.group === "committed" && sameSide(entry.old, body.change.old) && sameSide(entry.new, body.change.new));
          if (!found || found.entry.group === "conflicted") return fail({ code: "file_not_found", params: { path: "" }, message: "no such change" });
          if (found.error) return fail(found.error);
          const whole = found.whole ?? {};
          const at = `${body.left.branch}..${body.right.branch}`;
          return {
            type: "project_comparison_change",
            project: body.project,
            left: body.left,
            right: body.right,
            old: readSide(at, found.entry.old, found.bodies?.old, whole.old, hasHunk(found)),
            new: readSide(at, found.entry.new, found.bodies?.new, whole.new, hasHunk(found)),
            patch: found.patch === undefined ? null : { size: found.patch.length, kind: "text", media_type: null, text: found.patch, data: null },
          };
        }
        case "read_project_change": {
          const worktree = worktreeOf(body.worktree);
          const changes = git?.changes[worktree];
          const found = Array.isArray(changes)
            ? changes.find(
                ({ entry }) =>
                  entry.group === body.change.group && sameSide(entry.old, body.change.old) && sameSide(entry.new, body.change.new),
              )
            : undefined;
          if (!found || found.entry.group === "conflicted" || found.entry.group === "committed") {
            return fail({ code: "file_not_found", params: { path: "" }, message: "no such change" });
          }
          if (found.error) return fail(found.error);
          const whole = found.whole ?? {};
          const at = `${worktree} ${found.entry.group}`;
          return {
            type: "project_change",
            project: body.project,
            worktree: body.worktree ?? null,
            group: found.entry.group,
            head: null,
            old: readSide(at, found.entry.old, found.bodies?.old, whole.old, hasHunk(found)),
            new: readSide(at, found.entry.new, found.bodies?.new, whole.new, hasHunk(found)),
            patch: found.patch === undefined ? null : { size: found.patch.length, kind: "text", media_type: null, text: found.patch, data: null },
          };
        }
        case "read_project_change_bodies":
        case "read_project_comparison_change_bodies": {
          const asked = body.type === "read_project_change_bodies";
          const at = asked ? `${worktreeOf(body.worktree)} ${body.change.group} bodies` : `${body.left.branch}..${body.right.branch} bodies`;
          const changes = asked
            ? git?.changes[worktreeOf(body.worktree)]
            : git?.comparisons?.[`${body.left.branch}..${body.right.branch}`];
          const found = Array.isArray(changes)
            ? changes.find(
                ({ entry }) =>
                  entry.group !== "conflicted" &&
                  (!asked || entry.group === body.change.group) &&
                  sameSide(entry.old, body.change.old) &&
                  sameSide(entry.new, body.change.new),
              )
            : undefined;
          if (!found || found.entry.group === "conflicted") return fail({ code: "invalid_change", params: {}, message: "no patch to expand" });
          // The daemon reads no content for a link or a submodule, and refuses the whole request
          // rather than answering that side empty.
          const unsupported = notAFile(found.entry.old) ?? notAFile(found.entry.new);
          if (unsupported) return fail(unsupported);
          if (!found.expand) return fail({ code: "invalid_change", params: {}, message: "no patch to expand" });
          const { delay, error } = found.expand;
          if (delay) await new Promise((resolve) => setTimeout(resolve, delay));
          if (error) return fail(error);
          const whole = found.whole ?? {};
          /** A side as the bodies reply reads it. Unlike a diff reply, this one reads every present
           * file side whatever its source, so a side with no whole text here is a gap in the
           * fixture rather than one the daemon leaves empty. */
          const readWhole = (side: ChangeSide, content: string | undefined): SideRead => {
            if (side.state === "present" && side.kind === "file" && content === undefined) {
              refuseUnfillable(`${at} (${side.path}): the daemon reads this side's whole body, and the fixture carries none`);
            }
            return readSide(at, side, content === undefined ? undefined : text(content));
          };
          const sides = { old: readWhole(found.entry.old, whole.old), new: readWhole(found.entry.new, whole.new) };
          return asked
            ? { type: "project_change_bodies", project: body.project, worktree: body.worktree ?? null, group: body.change.group, ...sides }
            : { type: "project_comparison_change_bodies", project: body.project, left: body.left, right: body.right, ...sides };
        }
        case "list_dir": {
          const listed = scenario.directories?.[body.path];
          return { type: "dir_listing", ...(listed ? { path: body.path, entries: listed } : genericListing(body.path)) };
        }
        case "detect_directory_agent":
        case "probe_git_remote": {
          const detected = scenario.detections?.[body.type === "probe_git_remote" ? body.remote_url : body.path];
          if (detected === "pending") return new Promise<Event>(() => {});
          if (detected && "code" in detected) return fail(detected);
          return { type: "agent_detected", agent: detected?.agent ?? null };
        }
        case "list_agent_models": {
          const listed = scenario.agentModels?.[body.agent];
          if (listed === "pending") return new Promise<Event>(() => {});
          const { models, unavailable = null } = listed ?? { models: SAMPLE_MODELS[body.agent] };
          return { type: "agent_models", agent: body.agent, account: body.account ?? null, models, unavailable };
        }
        case "list_pages": {
          // As the real client's reply reaches the reducer, so a console session with no fixture
          // pages ends up listed as empty rather than not listed yet.
          const pages = store.getState().pages.get(body.console_session) ?? [];
          store.setState((state) =>
            state.pages.has(body.console_session)
              ? state
              : { pages: new Map(state.pages).set(body.console_session, pages) },
          );
          return { type: "page_list", console_session_id: body.console_session, pages };
        }
        case "create_console": {
          // As the daemon does, the new console reaches the store before the request is answered
          // (`coordinator.rs` broadcasts first), so a scenario can reach the first console's arrival.
          const { name, default_agent, icon } = body;
          const id = `console-${Date.now()}`;
          store.setState((state) => ({
            consoles: new Map(state.consoles).set(id, {
              id,
              name,
              workdir: `${HOME}/.octoboard/consoles/${id}`,
              default_agent,
              icon,
              created_at: Date.now(),
            }),
          }));
          return { type: "ack" };
        }
        default:
          return { type: "ack" };
      }
    },
    toastError: (message, session) => emitToast({ kind: "error", message, session }),
    toastNotice: (message) => emitToast({ kind: "notice", message }),
    onToast: (listener) => {
      toastListeners.add(listener);
      // StrictMode subscribes, unsubscribes and subscribes again; by the time this fires only the
      // last subscription is left.
      setTimeout(raiseScenarioToasts, TOAST_DELAY_MS);
      return () => void toastListeners.delete(listener);
    },
    dismissTrustPrompt: (session) =>
      store.setState((state) => ({ trustPrompts: state.trustPrompts.filter((p) => p.session !== session) })),
    reconnect: () => {},
    terminalUrl: (session) => terminalFixtureUrl(scenario.terminal ?? "output", session),
    terminalReconnectDelay: scenario.terminalRetriesImmediate ? () => 0 : undefined,
  };
}
