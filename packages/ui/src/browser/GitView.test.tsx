// @vitest-environment jsdom
import { act, useState } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import type { ProjectSourceInfo } from "../protocol";
import { openDetails, press } from "../components/detailsTestSupport";
import { change, changeViewFixture, mountRows } from "./browserTestSupport";
import { changeItem } from "./changes";
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


it("retains source Git stderr as structured diagnostics in the pane, which opens no error alert dialog", () => {
  const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
  onTestFinished(() => report.mockRestore());
  const detail = "fatal: unable to read /tmp/مراجعة\n    indented reason";
  const { container } = mountWorktreeNotice("/r", { ...source("/r", "/r"), git: null, git_error: detail });
  expect(report).not.toHaveBeenCalled();
  // The diagnostics are behind the Alert's Details button, not in the Alert.
  expect(container.querySelector("pre")).toBeNull();
  const dialog = openDetails(container);
  expect(dialog.querySelector("pre")?.textContent).toBe(detail);
  expect(dialog.querySelector("pre")?.getAttribute("dir")).toBe("ltr");
  expect(dialog.querySelectorAll("pre")).toHaveLength(1);
  expect(container.querySelector("[data-slot=alert-description] pre")).toBeNull();
  expect(container.querySelector("[role=alert]")).toBeNull();
});


it("keeps a pending comparison rich in visible prose and isolated plain text in its announcement", async () => {
  const branch = "feature/مراجعة-中文(2)";
  const base = "origin/أساس-中文(1)";
  const onBranchChange = vi.fn();
  const container = mountRows(
    <GitView
      source={{ state: "loaded", source: source("/r", "/r"), sentAt: 0 }}
      branches={{ state: "loaded", branches: [{ name: branch, commit: "c1" }], complete: true, sentAt: 0 }}
      branch={branch}
      onBranchChange={onBranchChange}
      uncommitted={{ state: "notCheckedOut", branch }}
      committed={{ state: "compare", branch, base, baseKind: "remote", comparison: { state: "loading" }, commits: undefined }}
      items={[changeItem(change("unstaged", "readme.md"))]}
      selectedChange={undefined}
      onOpen={() => {}}
      onRetry={() => {}}
      listRef={() => {}}
      selectorsRef={{ current: null }}
      changeView={changeViewFixture()}
      selectedSessionId={undefined}
    />,
  );
  const notice = [...container.querySelectorAll("p")].find((node) => node.textContent === `Comparing with ${base}…`)!;
  expect(notice).toBeDefined();
  const chip = notice.querySelector("[data-inline-chip]")!;
  expect(chip.textContent).toBe(base);
  expect(chip.getAttribute("dir")).toBe("ltr");
  expect(chip.querySelector("code")?.textContent).toBe(base);
  expect(chip.getAttribute("data-size")).toBe("xs");
  expect(chip.classList.contains("inline-chip--wrap")).toBe(true);
  const selector = container.querySelector('button[aria-haspopup="listbox"]')!;
  expect(selector.textContent).toContain(branch);
  expect(selector.textContent).not.toMatch(/[\u2066-\u2069]/u);
  expect(selector.querySelector("[data-inline-chip]")).toBeNull();
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  const announcement = container.querySelector('[role="status"].sr-only')!;
  expect(announcement.textContent).toBe(`Comparing with \u2068${base}\u2069…`);
  expect(announcement.children).toHaveLength(0);
  expect(onBranchChange).not.toHaveBeenCalled();
});

/** The Changes view with one part failed and the others listed. */
function mountPartFailure(props: Partial<ChangesViewProps>, onRetry = vi.fn()) {
  const container = mountRows(
    <GitView
      source={{ state: "loaded", source: source("/r", "/r"), sentAt: 0 }}
      branches={{ state: "loaded", branches: [{ name: "feature", commit: "c1" }], complete: true, sentAt: 0 }}
      branch="feature"
      onBranchChange={() => {}}
      uncommitted={{
        state: "listed",
        list: { state: "error", message: "Unable to read the index", unavailable: false, refreshing: false },
        worktree: { id: "w", root: "/r", main: true, head: "c1", branch: "feature", scope_present: true },
        own: true,
      }}
      committed={{ state: "none" }}
      items={[changeItem(change("staged", "readme.md"))]}
      selectedChange={undefined}
      onOpen={() => {}}
      onRetry={onRetry}
      listRef={() => {}}
      selectorsRef={{ current: null }}
      changeView={changeViewFixture()}
      selectedSessionId={undefined}
      {...props}
    />,
  );
  return { container, onRetry };
}

it("marks a part that could not be listed on the heading of its section, with no Alert and no standalone glyph", () => {
  const { container, onRetry } = mountPartFailure({});
  expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
  const heading = container.querySelector<HTMLElement>("[data-section-heading]")!;
  const mark = heading.parentElement!.querySelector<HTMLButtonElement>("button[aria-label]:not([data-section-heading])")!;
  expect(mark.getAttribute("aria-label")).toBe("Could not list the changes in this worktree");
  expect(heading.textContent).toContain("Staged");
  press(mark);
  const dialog = document.querySelector('[role="dialog"]')!;
  expect(dialog.textContent).toContain("Unable to read the index");
  act(() => [...dialog.querySelectorAll("button")].find((button) => button.textContent === "Try again")!.click());
  expect(onRetry).toHaveBeenCalledOnce();
});

/** The heading's band and the failure mark it carries, in a view with one part failed. */
function headingWithMark(onRetry = vi.fn()) {
  const { container } = mountPartFailure({}, onRetry);
  const band = container.querySelector<HTMLElement>("[data-section-heading]")!;
  const mark = band.parentElement!.querySelector<HTMLButtonElement>("[data-section-heading-control]")!;
  const key = (target: HTMLElement, name: string) => act(() => void target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true })));
  // Focus put on a heading from outside the list is sent to the first row, as the list does for anything
  // that is no row; from a row the walk's Up reaches it, as it does for the keyboard.
  const focusBand = () => {
    const row = container.querySelector<HTMLElement>('[role="row"][aria-label]')!;
    act(() => row.focus());
    key(row, "ArrowUp");
    expect(document.activeElement).toBe(band);
  };
  return { container, band, mark, key, focusBand, onRetry };
}

it("reaches the mark on a heading with Right from its band and goes back with Left, the mark being no tab stop", () => {
  const { band, mark, key, focusBand } = headingWithMark();
  expect(mark.tabIndex).toBe(-1);
  focusBand();
  key(band, "ArrowRight");
  expect(document.activeElement).toBe(mark);
  key(mark, "ArrowLeft");
  expect(document.activeElement).toBe(band);
  // Left from the band and Right from the mark have nowhere to go along the heading.
  key(band, "ArrowLeft");
  expect(document.activeElement).toBe(band);
  key(band, "ArrowRight");
  key(mark, "ArrowRight");
  expect(document.activeElement).toBe(mark);
});

it("does not take focus from a mouse press on the mark, which the list would answer by sending it to its first row", () => {
  const { band, mark, focusBand } = headingWithMark();
  focusBand();
  // The press down: a control without `preventFocusOnPress` takes focus here.
  act(() => void mark.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, button: 0, pointerId: 1, pointerType: "mouse" })));
  expect(document.activeElement).toBe(band);
});

it("gives focus back to the heading's band when the mark's popover closes with Escape or with Try again", async () => {
  const frames = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 80)));
  const { band, mark, key, focusBand, onRetry } = headingWithMark();
  focusBand();
  key(band, "ArrowRight");
  act(() => void mark.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  press(mark);
  const dialog = document.querySelector('[role="dialog"]')!;
  act(() => void dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await frames();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(document.activeElement).toBe(band);
  press(mark);
  act(() => [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === "Try again")!.click());
  await frames();
  expect(onRetry).toHaveBeenCalledOnce();
  expect(document.activeElement).toBe(band);
});

it("offers no Try again on a failure that is the state of the repository, where a retry gives the same answer", () => {
  const { container } = mountPartFailure({
    items: [changeItem(change("unstaged", "a.ts"))],
    uncommitted: { state: "notCheckedOut", branch: "feature" },
    committed: { state: "compare", branch: "feature", base: "main", baseKind: "remote", comparison: { state: "error", message: new DaemonRequestError("no common commit", "no_merge_base", { left: "main", right: "feature" }) }, commits: undefined },
  });
  const mark = container.querySelector<HTMLButtonElement>("[data-section-heading-control]")!;
  press(mark);
  const dialog = document.querySelector('[role="dialog"]')!;
  expect(dialog).not.toBeNull();
  expect([...dialog.querySelectorAll("button")].map((button) => button.textContent)).not.toContain("Try again");
});

it("puts both failed parts into one empty-state failure with one Try again and each part's words in Details, when nothing is listed", () => {
  const onRetry = vi.fn();
  const { container } = mountPartFailure({
    items: [],
    committed: { state: "compare", branch: "feature", base: "main", baseKind: "remote", comparison: { state: "error", message: "Unable to compare" }, commits: undefined },
  }, onRetry);
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(panel.querySelector("h2")?.textContent).toBe("Could not list the changes");
  expect(container.querySelector("[data-section-heading-control]")).toBeNull();
  expect([...panel.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Try again", "Details"]);
  const dialog = openDetails(panel);
  expect(dialog.textContent).toContain("The uncommitted changes could not be listed");
  expect(dialog.textContent).toContain("This branch's commits could not be compared");
  act(() => [...panel.querySelectorAll("button")].find((button) => button.textContent === "Try again")!.click());
  expect(onRetry).toHaveBeenCalledOnce();
});

it("keeps the heading of a section with no rows to carry the mark of its failure", () => {
  const { container } = mountPartFailure({ committed: { state: "compare", branch: "feature", base: "main", baseKind: "remote", comparison: { state: "error", message: "bad" }, commits: undefined } });
  expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
  const headings = [...container.querySelectorAll("[data-section-heading]")].map((heading) => heading.textContent);
  expect(headings).toEqual(["Staged1", "Committed0"]);
  expect(container.querySelectorAll("[data-section-heading-control]")).toHaveLength(2);
});
