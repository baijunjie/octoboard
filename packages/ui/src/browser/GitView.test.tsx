// @vitest-environment jsdom
import { act, useState } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import type { ProjectSourceInfo } from "../protocol";
import { changeViewFixture, mountRows } from "./browserTestSupport";
import { GitView, type ChangesViewProps } from "./GitView";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const source = (root: string, resolvedRoot: string): ProjectSourceInfo => ({
  project: "p",
  root,
  resolved_root: resolvedRoot,
  root_id: "r",
  git: {
    repository: "/r",
    common_dir: "/r/.git",
    worktree: "w",
    scope: "",
    worktrees: [],
  },
  git_error: null,
});

/** The Changes view of a branch checked out in another worktree, rooted at `worktreeRoot`. */
function mountWorktreeNotice(worktreeRoot: string, project: ProjectSourceInfo) {
  const writeText = vi.fn(async () => {});
  const props: ChangesViewProps = {
    branches: { state: "loaded", branches: [{ name: "feature", commit: "c1" }], complete: true, sentAt: 0 },
    branch: "feature",
    onBranchChange: () => {},
    uncommitted: {
      state: "listed",
      list: { state: "loaded", head: "c1", items: [], complete: true, refreshing: false },
      worktree: { id: "w2", root: worktreeRoot, main: false, head: "c1", branch: "feature", scope_present: true },
      own: false,
    },
    committed: { state: "none" },
    items: [],
    selectedChange: undefined,
    onOpen: () => {},
    onRetry: () => {},
    listRef: () => {},
    selectorsRef: { current: null },
    changeView: changeViewFixture(),
    selectedSessionId: undefined,
  };
  let uncheck = () => {};
  function Host() {
    const [uncommitted, setUncommitted] = useState(props.uncommitted);
    uncheck = () => setUncommitted({ state: "notCheckedOut", branch: "feature" });
    return <GitView source={{ state: "loaded", source: project, sentAt: 0 }} {...props} uncommitted={uncommitted} />;
  }
  const container = mountRows(<Host />, { clipboard: { writeText } });
  const button = container.querySelector<HTMLButtonElement>('p button[aria-label="Copy worktree path"]');
  return { container, button, writeText, uncheck: () => uncheck(), selector: props.selectorsRef };
}

async function copied(button: HTMLButtonElement | null): Promise<void> {
  button!.click();
  await act(async () => {});
}

it("copies a worktree inside the project's folder as the path relative to it", async () => {
  const { button, writeText } = mountWorktreeNotice("/work/app/.worktrees/feature", source("/work/app", "/work/app"));
  await copied(button);
  expect(writeText).toHaveBeenCalledExactlyOnceWith(".worktrees/feature");
});

it("takes the folder the project resolves to as one the worktree can be inside", async () => {
  const { button, writeText } = mountWorktreeNotice("/real/app/.worktrees/feature", source("/link/app", "/real/app"));
  await copied(button);
  expect(writeText).toHaveBeenCalledExactlyOnceWith(".worktrees/feature");
});

it("copies the absolute path of a worktree outside the project's folder", async () => {
  const { button, writeText } = mountWorktreeNotice("/work/app-2/feature", source("/work/app", "/work/app"));
  await copied(button);
  expect(writeText).toHaveBeenCalledExactlyOnceWith("/work/app-2/feature");
});

it("returns keyboard focus to the branch selector when its worktree row disappears", () => {
  const { button, uncheck, selector } = mountWorktreeNotice("/work/app/.worktrees/feature", source("/work/app", "/work/app"));
  act(() => button!.focus());
  expect(document.activeElement).toBe(button);
  act(() => uncheck());
  expect(button!.isConnected).toBe(false);
  expect(document.activeElement).toBe(selector.current?.querySelector("button"));
});


it("retains source Git stderr as structured diagnostics in both acknowledgement and the durable pane", () => {
  const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
  onTestFinished(() => report.mockRestore());
  const detail = "fatal: unable to read /tmp/مراجعة\n    indented reason";
  const { container } = mountWorktreeNotice("/r", { ...source("/r", "/r"), git: null, git_error: detail });
  expect(report).toHaveBeenCalledWith(expect.objectContaining({ message: detail, error: expect.objectContaining({ code: "git_unavailable", params: { detail } }) }));
  expect(container.querySelector("pre")?.textContent).toBe(detail);
  expect(container.querySelector("pre")?.getAttribute("dir")).toBe("ltr");
  expect(container.querySelectorAll("pre")).toHaveLength(1);
  expect(container.querySelector("[data-slot=alert-description] pre")).toBeNull();
  expect(container.querySelector("[role=alert]")).toBeNull();
});
