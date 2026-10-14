import { DaemonRequestError } from "../daemon-client";
import { createFixtureDaemon } from "../gallery/fixtureDaemon";
import type { Scenario } from "../gallery/scenario";
import type { Daemon } from "../store";
import { previewRepositories } from "./repositories";

const browseRequests = new Set([
  "get_project_source",
  "list_project_dir",
  "read_project_file",
  "list_project_changes",
  "read_project_change",
  "read_project_change_bodies",
  "list_project_branches",
  "compare_project_branches",
  "read_project_comparison_change",
  "read_project_comparison_change_bodies",
]);

/** The gallery's single repository fixtures are reused behind a project router; app state and
 * terminals still belong to one shared daemon. Every request stays inside these in-memory fakes. */
export function createPreviewDaemon(scenario: Scenario): Daemon {
  const daemon = createFixtureDaemon({ ...scenario, files: {}, git: null });
  const projects = new Map(
    [...previewRepositories].map(([id, repository]) => [
      id,
      createFixtureDaemon({
        ...scenario,
        files: repository.files,
        git: repository.git,
      }),
    ]),
  );
  return {
    ...daemon,
    request: async (body) => {
      if (!browseRequests.has(body.type)) return daemon.request(body);
      if (!("project" in body) || typeof body.project !== "string")
        throw new Error("Browse request needs a project");
      const repository = previewRepositories.get(body.project);
      const reader = projects.get(body.project);
      if (
        !repository ||
        !reader ||
        !daemon.store.getState().projects.has(body.project)
      )
        throw new DaemonRequestError(
          "Unknown preview project",
          "unknown_project",
          { project: body.project },
        );
      const worktree = repository.git.worktrees[0].id;
      if (
        "worktree" in body &&
        body.worktree !== undefined &&
        body.worktree !== worktree
      )
        throw new DaemonRequestError(
          "Worktree belongs to another preview project",
          "worktree_unavailable",
          { worktree: body.worktree },
        );
      const reply = await reader.request(body);
      if (reply.type === "project_source")
        return {
          ...reply,
          source: {
            ...reply.source,
            root: repository.root,
            resolved_root: repository.root,
            root_id: repository.rootId,
            git: reply.source.git && {
              ...reply.source.git,
              repository: body.project,
              common_dir: `${repository.root}/.git`,
            },
          },
        };
      if (reply.type === "project_dir")
        return {
          ...reply,
          worktree: "worktree" in body ? (body.worktree ?? null) : null,
          root_id: repository.rootId,
        };
      if (reply.type === "project_file" && body.type === "read_project_file") {
        const from = body.from ?? { kind: "live" };
        const historical = from.kind === "branch" || from.kind === "commit";
        if (
          from.kind === "branch" &&
          from.branch !== "main" &&
          from.branch !== repository.branch
        )
          throw new DaemonRequestError(
            "Unknown preview branch",
            "unknown_branch",
            { branch: from.branch },
          );
        if (from.kind === "commit" && from.commit !== repository.head)
          throw new DaemonRequestError(
            "Unknown preview revision",
            "file_not_found",
            { path: body.path },
          );
        const text = historical
          ? repository.committed[body.path]
          : reply.file.text;
        if (text === undefined || text === null)
          throw new DaemonRequestError(
            "File is absent in this revision",
            "file_not_found",
            { path: body.path },
          );
        return {
          ...reply,
          worktree: body.worktree ?? null,
          source:
            from.kind === "live"
              ? { kind: "live", root_id: repository.rootId, version: "1" }
              : from.kind === "index"
                ? { kind: "index", worktree, blob: repository.blob(text) }
                : {
                    kind: "commit",
                    commit: repository.head,
                    branch: from.kind === "branch" ? from.branch : null,
                    blob: repository.blob(text),
                  },
          file: {
            ...reply.file,
            text,
            size: new TextEncoder().encode(text).length,
          },
        };
      }
      return reply;
    },
  };
}
