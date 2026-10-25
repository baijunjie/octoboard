// @vitest-environment jsdom
import { posix } from "node:path";
import ts from "typescript";
import { describe, expect, it } from "vitest";
import { objectIdCheck } from "../gallery/fixtures/invariants";
import { sessionStatusKind } from "../sessionLabel";
import { previewRepositories } from "./repositories";
import { createPreviewDaemon } from "./daemon";
import { createPreviewScenario } from "./scenarios";
import type { TerminalScript } from "../gallery/fakeTerminal";

/** What a terminal output draws first, at the classic width. */
const screenOf = (output: string | TerminalScript | undefined) =>
  typeof output === "object" ? output.screen(80, 0) : output;
const daemon = () =>
  createPreviewDaemon(createPreviewScenario("changes", "en").scenario);
const primaryFiles: Record<string, string> = {
  "preview-admin": "src/domain/promotion.ts",
  "preview-merchant": "src/domain/availability.ts",
  "preview-customer": "src/components/Checkout.tsx",
  "preview-server": "src/meal-deal-orders.ts",
  "preview-mobile": "src/domain/redemption.ts",
  "preview-tablet": "src/domain/add-ons.ts",
  "preview-pos": "src/domain/checkout.ts",
  "preview-kitchen": "src/domain/print-job.ts",
};

describe("the website's eight independent repository fixtures", () => {
  it("gives every role a distinct business source and change list", () => {
    const sources = new Set<string>();
    const changes = new Set<string>();
    for (const [id, repository] of previewRepositories) {
      const source = repository.files[primaryFiles[id]];
      if (!source || !("text" in source)) throw new Error("Missing business source");
      sources.add(source.text!);
      const entries = repository.git.changes[repository.git.worktrees[0].id];
      if (!Array.isArray(entries)) throw new Error("Missing changes");
      changes.add(JSON.stringify(entries.map(({ entry }) => {
        if (!("new" in entry) || entry.new.state !== "present")
          throw new Error("Expected current business files");
        return entry.new.path;
      }).sort()));
    }
    expect(sources.size).toBe(8);
    expect(changes.size).toBe(8);
  });

  it.each([...previewRepositories])(
    "routes %s source, directories, files and Git to the same repository",
    async (id, repository) => {
      const api = daemon();
      const source = await api.request({
        type: "get_project_source",
        project: id,
      });
      expect(source).toMatchObject({
        type: "project_source",
        source: {
          project: id,
          root: repository.root,
          resolved_root: repository.root,
          root_id: repository.rootId,
          git: {
            repository: id,
            common_dir: `${repository.root}/.git`,
            worktree: repository.git.worktrees[0].id,
          },
        },
      });
      const root = await api.request({
        type: "list_project_dir",
        project: id,
        path: "",
      });
      expect(root.type).toBe("project_dir");
      if (root.type !== "project_dir") throw new Error("Missing directory");
      expect(root.root_id).toBe(repository.rootId);
      expect(root.entries.map((entry) => entry.name)).toContain("package.json");
      const manifest = await api.request({
        type: "read_project_file",
        project: id,
        path: "package.json",
      });
      expect(manifest.type).toBe("project_file");
      if (manifest.type !== "project_file") throw new Error("Missing file");
      expect(JSON.parse(manifest.file.text!).name).toBe(
        `@bistro/${id.slice(8)}`,
      );
      expect(manifest.source).toMatchObject({ root_id: repository.rootId });
      const listing = await api.request({
        type: "list_project_changes",
        project: id,
      });
      if (listing.type !== "project_changes")
        throw new Error("Missing changes");
      expect(
        listing.changes.some(
          (entry) =>
            "new" in entry &&
            entry.new.state === "present" &&
            entry.new.path === primaryFiles[id],
        ),
      ).toBe(true);
      const worktree = repository.git.worktrees[0].id;
      for (const change of listing.changes) {
        if (change.group === "conflicted" || change.group === "committed")
          throw new Error("Unexpected change");
        const requested = { ...change, group: change.group };
        const read = await api.request({
          type: "read_project_change",
          project: id,
          worktree,
          change: requested,
        });
        if (read.type !== "project_change" || read.new.state !== "present")
          throw new Error("Missing change body");
        const live = await api.request({
          type: "read_project_file",
          project: id,
          path: read.new.path,
          worktree,
        });
        if (live.type !== "project_file") throw new Error("Missing live file");
        expect(live.worktree).toBe(worktree);
        const fixture = repository.git.changes[worktree];
        if (!Array.isArray(fixture)) throw new Error("Invalid fixture");
        const stored = fixture.find((item) => item.entry === change)!;
        expect(live.file.text).toBe(stored.whole!.new);
        const lines = read.patch!.text!.split("\n");
        const hunk = lines.findIndex((line) => line.startsWith("@@ "));
        const body = lines.slice(hunk + 1, -1);
        const old = body
          .filter((line) => line[0] === " " || line[0] === "-")
          .map((line) => line.slice(1));
        const next = body
          .filter((line) => line[0] === " " || line[0] === "+")
          .map((line) => line.slice(1));
        expect(old.length ? old.join("\n") + "\n" : "").toBe(
          stored.whole!.old ?? "",
        );
        expect(next.join("\n") + "\n").toBe(live.file.text);
        const counts = /@@ -(\d+),(\d+) \+(\d+),(\d+) @@/.exec(lines[hunk])!;
        expect(Number(counts[2])).toBe(old.length);
        expect(Number(counts[4])).toBe(next.length);
        const bodies = await api.request({
          type: "read_project_change_bodies",
          project: id,
          worktree,
          change: requested,
          old:
            read.old.state === "present"
              ? { state: "present", source: read.old.source }
              : { state: "absent" },
          new: { state: "present", source: read.new.source },
        });
        expect(bodies).toMatchObject({
          type: "project_change_bodies",
          new: { file: { text: live.file.text } },
        });
      }
      expect(objectIdCheck(repository.git)).toEqual({
        conflicts: [],
        unchecked: [],
      });
    },
  );

  it("does not leak another project's files, worktrees or revisions after project switches", async () => {
    const api = daemon();
    for (const id of [...previewRepositories.keys(), "preview-admin"]) {
      const expected = previewRepositories.get(id)!.files[primaryFiles[id]];
      if (!expected || !("text" in expected)) throw new Error("Missing source file");
      const current = await api.request({
        type: "read_project_file",
        project: id,
        path: primaryFiles[id],
      });
      expect(current).toMatchObject({
        type: "project_file",
        project: id,
        path: primaryFiles[id],
        file: { text: expected.text },
      });
      for (const [other, path] of Object.entries(primaryFiles)) {
        if (other === id) continue;
        if (!previewRepositories.get(id)!.files[path])
          await expect(
            api.request({ type: "read_project_file", project: id, path }),
          ).rejects.toMatchObject({ code: "file_not_found" });
        await expect(
          api.request({
            type: "list_project_changes",
            project: id,
            worktree: `${other}-main`,
          }),
        ).rejects.toMatchObject({ code: "worktree_unavailable" });
      }
    }
    await expect(
      api.request({ type: "get_project_source", project: "unknown" }),
    ).rejects.toMatchObject({ code: "unknown_project" });
    await expect(
      api.request({
        type: "read_project_file",
        project: "preview-admin",
        path: "../server/package.json",
      }),
    ).rejects.toMatchObject({ code: "file_not_found" });
  });

  it.each([...previewRepositories])(
    "keeps %s index and committed reads honest",
    async (id, repository) => {
      const api = daemon();
      const path = primaryFiles[id];
      const index = await api.request({
        type: "read_project_file",
        project: id,
        path,
        from: { kind: "index" },
      });
      expect(index).toMatchObject({
        type: "project_file",
        source: { kind: "index", worktree: repository.git.worktrees[0].id },
      });
      const committed = await api.request({
        type: "read_project_file",
        project: id,
        path,
        from: { kind: "branch", branch: "main" },
      });
      expect(committed).toMatchObject({
        type: "project_file",
        file: { text: repository.committed[path] },
        source: { kind: "commit", commit: repository.head },
      });
      expect(committed).not.toEqual(index);
      await expect(
        api.request({
          type: "read_project_file",
          project: id,
          path,
          from: { kind: "branch", branch: "unknown" },
        }),
      ).rejects.toMatchObject({ code: "unknown_branch" });
    },
  );

  it.each([...previewRepositories])(
    "gives %s connected source imports and complete directory ancestry",
    (id, repository) => {
      const files = Object.entries(repository.files).filter(
        (entry): entry is [string, { text: string }] => "text" in entry[1],
      );
      expect(files.length).toBeGreaterThanOrEqual(10);
      expect(files.length).toBeLessThanOrEqual(20);
      for (const [path, file] of files) {
        expect(file.text.endsWith("\n"), `${id}/${path} final newline`).toBe(
          true,
        );
        const parent = posix.dirname(path);
        if (parent !== ".")
          expect(repository.files[parent], `${path} parent`).toMatchObject({
            dir: true,
          });
        if (!/\.[tj]sx?$/.test(path)) continue;
        const parsed = ts.createSourceFile(
          path,
          file.text,
          ts.ScriptTarget.Latest,
          true,
        );
        const result = ts.transpileModule(file.text, {
          fileName: path,
          reportDiagnostics: true,
          compilerOptions: {
            jsx: ts.JsxEmit.ReactJSX,
            target: ts.ScriptTarget.ES2022,
          },
        });
        expect(
          result.diagnostics?.filter(
            (item) => item.category === ts.DiagnosticCategory.Error,
          ),
          path,
        ).toEqual([]);
        for (const statement of parsed.statements) {
          if (
            !ts.isImportDeclaration(statement) ||
            !ts.isStringLiteral(statement.moduleSpecifier)
          )
            continue;
          const imported = statement.moduleSpecifier.text;
          if (!imported.startsWith(".")) continue;
          const resolved = posix.normalize(
            posix.join(posix.dirname(path), imported),
          );
          expect(
            [resolved, `${resolved}.ts`, `${resolved}.tsx`].some(
              (name) => repository.files[name],
            ),
            `${id}/${path}: ${imported}`,
          ).toBe(true);
        }
      }
    },
  );

  it.each([...previewRepositories])(
    "shows %s's feature branch as uncommitted changes plus what it committed since it forked from the main working copy's branch",
    async (id, repository) => {
      const api = daemon();
      const source = await api.request({ type: "get_project_source", project: id });
      if (source.type !== "project_source" || !source.source.git)
        throw new Error("Missing source");
      const { worktree, worktrees } = source.source.git;
      expect(worktrees.find((w) => w.id === worktree)).toMatchObject({
        main: false,
        branch: repository.branch,
      });
      expect(worktrees.find((w) => w.main)).toMatchObject({ branch: "main" });
      const compared = await api.request({
        type: "compare_project_branches",
        project: id,
        left: "main",
        right: repository.branch,
      });
      if (compared.type !== "project_comparison")
        throw new Error("Missing comparison");
      expect(compared.changes).toHaveLength(1);
      expect(compared.base).not.toBe(compared.right.commit);
      const commits = await api.request({
        type: "list_project_commits",
        project: id,
        base: compared.base,
        tip: compared.right.commit,
      });
      if (commits.type !== "project_commits")
        throw new Error("Missing commits");
      expect(commits.commits.map((commit) => commit.changes)).toEqual([
        compared.changes,
      ]);
      const main = worktrees.find((w) => w.main)!;
      await expect(
        api.request({ type: "list_project_changes", project: id, worktree: main.id }),
      ).resolves.toMatchObject({ type: "project_changes", changes: [] });
    },
  );

  it("preserves all scenes, the report bridge marker and project branch metadata", () => {
    for (const lang of ["en", "zh-Hans"] as const)
      for (const scene of ["overview", "focus", "changes"] as const) {
        const { scenario } = createPreviewScenario(scene, lang);
        expect(scenario.steps).toHaveLength(scene === "changes" ? 4 : 1);
        expect(scenario.files).toBeUndefined();
        expect(scenario.git).toBeUndefined();
        expect(
          scenario.state!.pages!.get("preview-coordinator")![0].html,
        ).toContain("data-website-preview-report");
        for (const [id, repo] of previewRepositories)
          expect(scenario.state!.gitStatuses!.get(id)?.branch).toBe(
            repo.branch,
          );
      }
  });

  it.each(["en", "zh-Hans"] as const)(
    "keeps one ongoing workspace behind every %s view",
    (language) => {
      const overview = createPreviewScenario("overview", language);
      for (const scene of ["overview", "focus", "changes"] as const) {
        const { scenario, output } = createPreviewScenario(scene, language);
        const sessions = scenario.state!.sessions!;
        expect(
          Object.fromEntries(
            [...sessions].map(([id, session]) => [id, sessionStatusKind(session)]),
          ),
        ).toEqual({
          "preview-coordinator": "working",
          "preview-admin-session": "idle",
          "preview-merchant-session": "working",
          "preview-customer-session": "working",
          "preview-server-session": "working",
          "preview-review": "background",
          "preview-order-checks": "waiting_user",
          "preview-mobile-session": "working",
          "preview-tablet-session": "working",
          "preview-pos-session": "idle",
          "preview-kitchen-session": "working",
        });
        expect(sessions.get("preview-review")).toMatchObject({
          status: "idle",
          background_tasks: [
            { type: "shell", description: "pnpm test --watch" },
          ],
        });
        expect(screenOf(output("preview-review"))).toContain("pnpm test --watch");
        for (const id of sessions.keys()) {
          expect(output(id)).toBeDefined();
          expect(screenOf(output(id))).toBe(screenOf(overview.output(id)));
        }
        expect(scenario.state!.pages!.get("preview-coordinator")![0].html).toBe(
          overview.scenario.state!.pages!.get("preview-coordinator")![0].html,
        );
      }
    },
  );
});
