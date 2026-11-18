// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it } from "vitest";

import type { ChangeSide, ClientRequestBody, Event, SideRead } from "../../protocol";
import { DaemonProvider, type Daemon } from "../../store";
import { testDaemon } from "../../test/testDaemon";
import { changeItem, type ChangeItem } from "./changes";
import { useChangeReader, type ChangeOrigin, type ChangeReader } from "./useChangeReader";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Pending = { body: ClientRequestBody; resolve: (event: Event) => void; reject: (err: Error) => void };

const leftOver: Pending[] = [];
const roots: Root[] = [];

/** A daemon whose replies the test hands out itself. */
function controlledDaemon() {
  const pending: Pending[] = [];
  const daemon = testDaemon({
    state: { connectionState: "open", snapshotEpoch: 1 },
    request: (body) =>
      new Promise((resolve, reject) => {
        const request = { body, resolve, reject };
        pending.push(request);
        leftOver.push(request);
      }),
  });
  return { daemon, pending };
}

function mountReader(daemon: Daemon): { current: () => ChangeReader } {
  let latest!: ChangeReader;
  const Probe = () => {
    latest = useChangeReader("p");
    return null;
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  act(() => root.render(<DaemonProvider value={daemon}><Probe /></DaemonProvider>));
  return { current: () => latest };
}

afterEach(async () => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  await act(async () => leftOver.splice(0).forEach((request) => request.reject(new Error("test over"))));
  document.body.replaceChildren();
});

const OWN: ChangeOrigin = { kind: "worktree", worktree: undefined };
const present: ChangeSide = { state: "present", path: "a.ts", kind: "file", source: { kind: "index", worktree: "w", blob: "b" } };
/** A side of a state a newer daemon names: the wire's tagged unions are open, which the types do not show. */
const newerSide = { state: "from_a_newer_daemon" } as unknown as ChangeSide;
const newerRead = { state: "from_a_newer_daemon" } as unknown as SideRead;
const itemOf = (old: ChangeSide, next: ChangeSide): ChangeItem => changeItem({ group: "unstaged", old, new: next });

const reply = (old: SideRead, next: SideRead): Event => ({
  type: "project_change",
  id: null,
  project: "p",
  worktree: null,
  group: "unstaged",
  head: null,
  old,
  new: next,
  patch: { size: 1, kind: "text", media_type: null, text: "-a\n", data: null },
});

it("asks for a change whose side is of a state this build does not know as one whose side is absent, and a known side as it was listed", () => {
  const { daemon, pending } = controlledDaemon();
  const reader = mountReader(daemon);
  expect(() => act(() => reader.current().open(itemOf(present, newerSide), OWN))).not.toThrow();
  expect(pending.map((request) => request.body)).toMatchObject([
    { type: "read_project_change", change: { group: "unstaged", old: { state: "present", path: "a.ts" }, new: { state: "absent" } } },
  ]);
  expect(JSON.stringify(pending[0]!.body)).not.toContain("from_a_newer_daemon");
});

it("shows a side the daemon read in a state this build does not know as absent, beside the side it knows", async () => {
  const { daemon, pending } = controlledDaemon();
  const reader = mountReader(daemon);
  act(() => reader.current().open(itemOf(present, newerSide), OWN));
  await act(async () => pending.splice(0)[0]!.resolve(reply({ ...present, file: null, lines: null }, newerRead)));
  const content = reader.current().subject?.content;
  expect(content?.state).toBe("change");
  if (content?.state !== "change") return;
  expect(content.change.old).toMatchObject({ state: "present", path: "a.ts" });
  expect(content.change.new).toEqual({ state: "absent" });
});

it("shows a listed side of a state this build does not know as absent when the change cannot be read", async () => {
  const { daemon, pending } = controlledDaemon();
  const reader = mountReader(daemon);
  act(() => reader.current().open(itemOf(newerSide, present), OWN));
  await act(async () => pending.splice(0)[0]!.reject(new Error("could not read")));
  const content = reader.current().subject?.content;
  expect(content?.state).toBe("change");
  if (content?.state !== "change") return;
  expect(content.change.unavailable).toBeDefined();
  expect(content.change.old).toEqual({ state: "absent" });
  expect(content.change.new).toMatchObject({ state: "present", path: "a.ts" });
});

it("offers no whole bodies to expand for a change with a side of a state this build does not know", async () => {
  const { daemon, pending } = controlledDaemon();
  const reader = mountReader(daemon);
  act(() => reader.current().open(itemOf(present, newerSide), OWN));
  await act(async () => pending.splice(0)[0]!.resolve(reply({ ...present, file: null, lines: null }, newerRead)));
  const content = reader.current().subject?.content;
  expect(content?.state === "change" && content.change.loadBodies).toBeFalsy();
});
