import {
  addition,
  textChange,
  type FixtureGit,
} from "../../gallery/fixtures/changeFixtures";
import type { FixtureFiles } from "../../gallery/fixtures/projectFiles";
import { admin } from "./admin";
import { merchant } from "./merchant";
import { customer } from "./customer";
import { server } from "./server";
import type { RepositoryExample } from "./types";

/** Full-context patches keep the display fixture and its expandable whole-file bodies identical.
 * The LCS preserves unchanged lines instead of showing the entire file as a replacement. */
function patch(path: string, before: string, after: string): string {
  const old = before.slice(0, -1).split("\n");
  const next = after.slice(0, -1).split("\n");
  const lengths = Array.from({ length: old.length + 1 }, () =>
    Array<number>(next.length + 1).fill(0),
  );
  for (let i = old.length - 1; i >= 0; i--)
    for (let j = next.length - 1; j >= 0; j--)
      lengths[i][j] =
        old[i] === next[j]
          ? lengths[i + 1][j + 1] + 1
          : Math.max(lengths[i + 1][j], lengths[i][j + 1]);
  const lines = [
    `diff --git a/${path} b/${path}`,
    `--- a/${path}`,
    `+++ b/${path}`,
    `@@ -1,${old.length} +1,${next.length} @@`,
  ];
  let i = 0,
    j = 0;
  while (i < old.length || j < next.length) {
    if (i < old.length && j < next.length && old[i] === next[j]) {
      lines.push(` ${old[i++]}`);
      j++;
    } else if (
      i < old.length &&
      (j === next.length || lengths[i + 1][j] >= lengths[i][j + 1])
    )
      lines.push(`-${old[i++]}`);
    else lines.push(`+${next[j++]}`);
  }
  return lines.join("\n") + "\n";
}

export interface PreviewRepository {
  project: string;
  root: string;
  rootId: string;
  branch: string;
  head: string;
  files: FixtureFiles;
  committed: Record<string, string>;
  blob: (content: string) => string;
  git: FixtureGit;
}

function repository(
  example: RepositoryExample,
  number: number,
): PreviewRepository {
  const project = `preview-${example.id}`;
  const root = `/demo/bistro/${example.id}`;
  const worktree = `${project}-main`;
  const head = `c${number.toString(16)}`.padEnd(40, "0");
  const blobs = new Map<string, string>();
  // Opaque fixture identities, shared by identical bodies; no real Git or filesystem is involved.
  const blob = (content: string) => {
    if (!blobs.has(content))
      blobs.set(
        content,
        `b${number.toString(16)}${blobs.size.toString(16).padStart(38, "0")}`,
      );
    return blobs.get(content)!;
  };
  const files: FixtureFiles = { "": { dir: true } };
  for (const [path, text] of Object.entries(example.files)) {
    files[path] = { text };
    const parts = path.split("/");
    for (let depth = 1; depth < parts.length; depth++)
      files[parts.slice(0, depth).join("/")] = { dir: true };
  }
  const committed = { ...example.files, ...example.edited };
  for (const path of example.added) delete committed[path];
  const staged = [
    ...Object.entries(example.edited).map(([path, before]) =>
      textChange(
        "staged",
        path,
        { kind: "commit", commit: head, branch: null, blob: blob(before) },
        { kind: "index", worktree, blob: blob(example.files[path]) },
        {
          old: before,
          new: example.files[path],
          patch: patch(path, before, example.files[path]),
        },
      ),
    ),
    ...example.added.map((path) =>
      addition(
        "staged",
        path,
        { kind: "index", worktree, blob: blob(example.files[path]) },
        example.files[path],
      ),
    ),
  ];
  return {
    project,
    root,
    rootId: `${project}-root`,
    branch: example.branch,
    head,
    files,
    committed,
    blob,
    git: {
      worktrees: [
        {
          id: worktree,
          root,
          main: true,
          head,
          branch: example.branch,
          scope_present: true,
        },
      ],
      changes: { [worktree]: staged },
      branches: [
        { name: "main", commit: head },
        { name: example.branch, commit: head },
      ],
    },
  };
}

export const previewRepositories: ReadonlyMap<string, PreviewRepository> =
  new Map(
    [admin, merchant, customer, server].map((example, index) => {
      const built = repository(example, index + 1);
      return [built.project, built];
    }),
  );
