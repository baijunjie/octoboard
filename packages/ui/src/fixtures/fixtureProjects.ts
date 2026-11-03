// What the fixture daemon answers for the projects: adding, changing and removing one, the actions the host carries out for
// it, the branch switch and the restore that change its files, and its shell terminals.
import type { ReplyOf } from "../requests";
import { FOLLOWING_GIT_WORKFLOW } from "../git/gitWorkflow";
import { type ProjectAccount, type Project, type ClientRequestBody, type ShellTerminal, type GitWorkflowOverride, type GitWorkflowOverrideChanges } from "../protocol";
import { scopeCause, type SourceUnavailableCause } from "../errors/causes";
import { createGitReplies } from "./fixtureGit";
import { refuseUnfillable } from "./banner";
import { projectOf } from "./builders";
import { permissionDenied, sourceUnavailable, type FixtureError } from "./sampleFiles";
import { genericListing } from "./fixtureFiles";
import type { ProcessLedger } from "./fixtureProcesses";
import { neverReplied, normalizedPath, trimmedOrNull, changedPerAgent, changedLaunch, normalizeTags, type Ack, type FixtureContext, type Handlers } from "./fixtureContext";

/** What the daemon sends for the project's place in another worktree when it cannot be used, by why (the worktree's
 * `scope_cause`): its own text for a folder that is not there, one reached through a link and one that is not a folder,
 * and the system's for any other failure to read it. A read the system denied is no `source_unavailable` and has its own
 * refusal (`permissionDenied`). */
const SCOPE_UNAVAILABLE: Record<SourceUnavailableCause, string> = {
  missing: "the project's directory is not in this worktree",
  link: "the path to the project's directory goes through a link in this worktree",
  not_a_directory: "not a directory",
  other: "Input/output error (os error 5)",
};

/** The system's text for a read it denied. */
const PERMISSION_DENIED_DETAIL = "Permission denied (os error 13)";

/** Mutable project account and launch settings on top of the project they change, retaining absent-versus-cleared values. */
function projectFields(current: Pick<Project, "accounts" | "launch">, body: Extract<ClientRequestBody, { type: "add_project" | "update_project" }>): Partial<Project> & Pick<Project, "accounts" | "launch"> {
  return {
    ...(body.default_agent !== undefined && { default_agent: body.default_agent }),
    accounts: changedPerAgent(current.accounts, body.accounts, (_, account): ProjectAccount => account ?? { kind: "inherit" }),
    launch: changedPerAgent(current.launch, body.launch, changedLaunch),
  };
}

/** A project's own git workflow settings after a request's: per item, absent keeps it, `null` goes back to following the
 * application-wide value, and a value becomes the project's own. */
function projectGitWorkflow(current: GitWorkflowOverride, changes: GitWorkflowOverrideChanges | undefined): GitWorkflowOverride {
  return { ...current, ...changes };
}

/** The replies about projects and their shell terminals. */
export function createProjectReplies({ scenario, store, fail, git, restored, startedShellTerminals, refuseUnknownProject, refuseLocked }: FixtureContext, ledger: ProcessLedger): Handlers {
  let createdProjectCount = 0;
  let createdShellTerminalCount = 0;
  /** A project action the host would carry out: refused for an unknown project, failed with the scenario's error when it
   * sets one, and acknowledged otherwise. */
  const projectAction = (project: string, error: FixtureError | undefined): Ack => {
    refuseUnknownProject(project);
    if (error) return fail(error);
    return { type: "ack", id: null };
  };

  const openPath = (body: Extract<ClientRequestBody, { type: "open_project_path" | "reveal_project_path" }>): Ack => {
    return projectAction(body.project, scenario.projectPathActionError);
  };

  const detectAgent = (body: Extract<ClientRequestBody, { type: "detect_directory_agent" | "probe_git_remote" }>): ReplyOf<"detect_directory_agent"> | Promise<never> => {
    const detected = scenario.detections?.[body.type === "probe_git_remote" ? body.remote_url : body.path];
    if (detected === "pending") return neverReplied();
    if (detected && "code" in detected) return fail(detected);
    return { type: "agent_detected", id: null, agent: detected?.agent ?? null };
  };

  return {
    add_project: (body) => {
      const current = store.getState();
      if (!current.consoles.has(body.console_id)) return fail({ code: "unknown_console", params: { console: body.console_id }, message: `unknown console ${body.console_id}` });
      const given = body.path ?? (body.source === "git" ? current.settings.default_clone_dir : "");
      const parent = normalizedPath(given.trim());
      if (!parent.startsWith("/")) return fail({ code: "path_not_absolute", params: { path: given }, message: `${given} is not an absolute path` });
      const remote = body.remote_url?.trim().replace(/\/+$/, "");
      const repository = remote?.split(/[/:]/).at(-1)?.replace(/\.git$/, "");
      if (body.source === "git" && !repository) return fail({ code: "field_required", params: { field: "remote_url" }, message: "remote_url is required" });
      const listedParent = scenario.directories?.[parent];
      if (listedParent && !Array.isArray(listedParent)) return fail(listedParent);
      const directories = body.source === "parent"
        ? (listedParent ?? genericListing(parent).entries).filter((entry) => entry.is_git_repo).map((entry) => normalizedPath(entry.path))
        : [body.source === "git" ? normalizedPath(`${parent}/${repository}`) : parent];
      if (!directories.length) return fail({ code: "no_repositories_found", params: { path: parent }, message: `no repositories found in ${parent}` });
      const projects = new Map(current.projects);
      const blank = projectOf("", body.console_id, "");
      let added = 0;
      for (const path of directories) {
        if ([...projects.values()].some((project) => project.console_id === body.console_id && project.path === path)) continue;
        const detection = scenario.detections?.[body.source === "git" ? body.remote_url! : path] ?? scenario.detections?.[given];
        const project: Project = {
          id: `project-gallery-${++createdProjectCount}`,
          console_id: body.console_id,
          host_id: "local",
          source: body.source,
          path,
          name: directories.length === 1 && body.name?.trim() || path.split("/").at(-1) || path,
          default_agent: body.default_agent ?? (body.detect_default_agent !== false && detection && typeof detection === "object" && "agent" in detection ? detection.agent : null),
          remote_url: body.source === "git" ? body.remote_url ?? null : null,
          trust_consent: false,
          pinned: false,
          tags: normalizeTags(body.tags ?? []),
          description: directories.length === 1 ? trimmedOrNull(body.description) : null,
          ...projectFields(blank, body),
          git_workflow: projectGitWorkflow(FOLLOWING_GIT_WORKFLOW, body.git_workflow),
        };
        projects.set(project.id, project);
        added++;
      }
      if (!added) return fail({ code: "all_projects_already_added", params: {}, message: "every directory found is already associated with this console" });
      store.setState({ projects });
      return { type: "ack", id: null };
    },
    update_project: (body) => {
      const current = store.getState().projects.get(body.project);
      if (!current) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
      if (body.name !== undefined && !body.name.trim()) return fail({ code: "field_required", params: { field: "name" }, message: "name is required" });
      const project = { ...current, ...projectFields(current, body), git_workflow: projectGitWorkflow(current.git_workflow, body.git_workflow),
        ...(body.name !== undefined && body.name.trim() !== current.name.trim() && { name: body.name.trim() }),
        ...(body.description !== undefined && trimmedOrNull(body.description) !== trimmedOrNull(current.description) && { description: trimmedOrNull(body.description) }),
        ...(body.tags !== undefined && { tags: normalizeTags(body.tags) }),
        ...(body.pinned !== undefined && { pinned: body.pinned }),
      };
      store.setState((state) => ({ projects: new Map(state.projects).set(project.id, project) }));
      return { type: "ack", id: null };
    },
    delete_project: (body) => {
      refuseUnknownProject(body.project);
      refuseLocked([...store.getState().sessions.values()].filter((session) => session.project_id === body.project));
      if (body.trash_directory && scenario.projectDeleteError) return fail(scenario.projectDeleteError);
      return { type: "ack", id: null };
    },
    inspect_project_removal: (body) => {
      refuseUnknownProject(body.project);
      const found = scenario.projectRemovalBranches?.[body.project] ?? [];
      if ("code" in found) return fail(found);
      // The project's processes its removal stops, whatever it was asked to remove with them.
      const processes = ledger.projectTrees(body.project);
      return Array.isArray(found) ? { type: "project_removal_inspected", id: null, items: found, processes } : { type: "project_removal_inspected", id: null, ...found, processes };
    },
    switch_project_branch: (body) => {
      refuseUnknownProject(body.project);
      const result = scenario.projectBranchSwitches?.[body.project]?.[body.branch];
      if (!result) return refuseUnfillable(`switch_project_branch (${body.project}, ${body.branch}): no post-switch repository, files and badge supplied`);
      if ("code" in result) return fail(result);
      restored.files.set(body.project, result.files);
      restored.git.set(body.project, createGitReplies(result.git, fail));
      store.setState((state) => ({ gitStatuses: new Map(state.gitStatuses).set(body.project, result.status) }));
      return { type: "ack", id: null };
    },
    restore_project_changes: (body) => {
      refuseUnknownProject(body.project);
      const result = scenario.projectRestores?.[body.project];
      if (!result) return refuseUnfillable(`restore_project_changes (${body.project}): no post-restore repository and files supplied`);
      if ("code" in result) return fail(result);
      restored.files.set(body.project, result.files);
      restored.git.set(body.project, createGitReplies(result.git, fail));
      return { type: "ack", id: null };
    },
    // Native path operations are acknowledged without launching anything on the host.
    open_project_path: openPath,
    reveal_project_path: openPath,
    open_project_directory: (body) => {
      return projectAction(body.project, scenario.projectDirectoryOpenError);
    },
    open_project_terminal: (body) => {
      return projectAction(body.project, scenario.projectTerminalOpenError);
    },
    push_project_branch: (body) => {
      return projectAction(body.project, scenario.projectPushError);
    },
    detect_directory_agent: detectAgent,
    probe_git_remote: detectAgent,
    create_shell_terminal: (body) => {
      const project = store.getState().projects.get(body.project);
      if (!project) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
      if (scenario.shellTerminalStart === "pending") return neverReplied();
      if (scenario.shellTerminalStartError) return fail(scenario.shellTerminalStartError);
      // A project's place in another worktree is that worktree's root joined with the repository's `scope`, as
      // `get_project_source` builds the project's own: the refusal names that place, and a terminal started there starts in it.
      const target = body.worktree === undefined ? undefined : git?.worktrees.find((worktree) => worktree.id === body.worktree);
      if (body.worktree !== undefined && !target) return fail({ code: "worktree_unavailable", params: { worktree: body.worktree }, message: `worktree ${body.worktree} is gone` });
      const placeIn = (worktree: { root: string }) => (git?.scope ? `${worktree.root}/${git.scope}` : worktree.root);
      // `scope_present` is false when the project's folder in that worktree cannot be used; the worktree's `scope_cause`
      // says why, and the refusal carries that cause with the daemon's own wording of it, as the daemon's one check gives both;
      // a read the system denied is refused as `permission_denied` instead, with the path and the system's text.
      if (target && !target.scope_present) {
        const cause = scopeCause(target.scope_cause);
        if (cause === "permission_denied") return fail(permissionDenied(placeIn(target), PERMISSION_DENIED_DETAIL));
        return fail(sourceUnavailable(placeIn(target), cause, SCOPE_UNAVAILABLE[cause]));
      }
      const shellTerminal: ShellTerminal = {
        id: `shell-gallery-${++createdShellTerminalCount}`,
        project_id: project.id,
        worktree: body.worktree ?? null,
        cwd: [target ? placeIn(target) : project.path, ...(body.path ? [body.path] : [])].join("/"),
        title: "zsh",
        renamed: false,
        state: "running",
        busy: false,
        created_at: Date.now(),
      };
      startedShellTerminals.add(shellTerminal.id);
      store.setState((state) => ({ shellTerminals: new Map(state.shellTerminals).set(shellTerminal.id, shellTerminal) }));
      return { type: "shell_terminal_created", id: null, shell_terminal: shellTerminal };
    },
    rename_shell_terminal: (body) => {
      const terminal = store.getState().shellTerminals.get(body.shell_terminal);
      if (!terminal) return fail({ code: "unknown_shell_terminal", params: { shell_terminal: body.shell_terminal }, message: `unknown shell terminal ${body.shell_terminal}` });
      const title = body.title.trim();
      if (!title) return fail({ code: "field_required", params: { field: "title" }, message: "title is required" });
      store.setState((state) => ({ shellTerminals: new Map(state.shellTerminals).set(terminal.id, { ...terminal, title, renamed: true }) }));
      return { type: "ack", id: null };
    },
    close_shell_terminal: (body) => {
      if (!store.getState().shellTerminals.has(body.shell_terminal)) return fail({ code: "unknown_shell_terminal", params: { shell_terminal: body.shell_terminal }, message: `unknown shell terminal ${body.shell_terminal}` });
      if (scenario.shellTerminalClose === "pending") return neverReplied();
      store.setState((state) => ({ shellTerminals: new Map([...state.shellTerminals].filter(([id]) => id !== body.shell_terminal)) }));
      return { type: "ack", id: null };
    },
  };
}
