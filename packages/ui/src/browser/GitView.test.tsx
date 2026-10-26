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
function mountWorktreeNotice(worktreeRoot: string, project: ProjectSourceInfo, own = false, complete = true) {
  const writeText = vi.fn(async () => {});
  const props: ChangesViewProps = {
    branches: { state: "loaded", branches: [{ name: "feature", commit: "c1" }], complete: true, sentAt: 0 },
    branch: "feature",
    onBranchChange: () => {},
    uncommitted: {
      state: "listed",
      list: { state: "loaded", head: "c1", items: [], complete, refreshing: false },
      worktree: { id: "w2", root: worktreeRoot, main: false, head: "c1", branch: "feature", scope_present: true },
      own,
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
  let finish = () => {};
  function Host() {
    const [uncommitted, setUncommitted] = useState(props.uncommitted);
    uncheck = () => setUncommitted({ state: "notCheckedOut", branch: "feature" });
    finish = () => setUncommitted((current) => (current.state === "listed" ? { ...current, list: { state: "loaded", head: "c1", items: [], complete: true, refreshing: false } } : current));
    return <GitView source={{ state: "loaded", source: project, sentAt: 0 }} {...props} uncommitted={uncommitted} />;
  }
  const container = mountRows(<Host />, { clipboard: { writeText } });
  const button = container.querySelector<HTMLButtonElement>('p button[aria-label="Copy worktree path"]');
  return { container, button, writeText, uncheck: () => uncheck(), finish: () => finish(), selector: props.selectorsRef };
}

async function copied(button: HTMLButtonElement | null): Promise<void> {
  button!.click();
  await act(async () => {});
}

it.each([
  ["a worktree inside the project's folder as the path relative to it", "/work/app/.worktrees/feature", source("/work/app", "/work/app"), ".worktrees/feature"],
  ["a worktree inside the folder the project resolves to as the path relative to it", "/real/app/.worktrees/feature", source("/link/app", "/real/app"), ".worktrees/feature"],
  ["the absolute path of a worktree outside the project's folder", "/work/app-2/feature", source("/work/app", "/work/app"), "/work/app-2/feature"],
])("copies %s", async (_name, worktreePath, project, expected) => {
  const { button, writeText } = mountWorktreeNotice(worktreePath, project);
  await copied(button);
  expect(writeText).toHaveBeenCalledExactlyOnceWith(expected);
});

it("returns keyboard focus to the branch selector when its worktree row disappears", () => {
  const { button, uncheck, selector } = mountWorktreeNotice("/work/app/.worktrees/feature", source("/work/app", "/work/app"));
  act(() => button!.focus());
  expect(document.activeElement).toBe(button);
  act(() => uncheck());
  expect(button!.isConnected).toBe(false);
  expect(document.activeElement).toBe(selector.current?.querySelector("button"));
});

it("returns keyboard focus to the branch selector when the own working copy's cut-short mark goes with its line", () => {
  const { container, finish, selector } = mountWorktreeNotice("/work/app", source("/work/app", "/work/app"), true, false);
  const mark = container.querySelector<HTMLButtonElement>('button[aria-label="Not every uncommitted change is listed"]')!;
  act(() => mark.focus());
  expect(document.activeElement).toBe(mark);
  act(() => finish());
  expect(mark.isConnected).toBe(false);
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
  const chip = notice.querySelector("[data-inline-reference]")!;
  expect(chip.textContent).toBe(base);
  expect(chip.getAttribute("dir")).toBe("ltr");
  expect(chip.querySelector("code")?.textContent).toBe(base);
  expect(chip.getAttribute("data-size")).toBe("xs");
  expect(chip.classList.contains("inline-reference--wrap")).toBe(true);
  const selector = container.querySelector('button[aria-haspopup="listbox"]')!;
  expect(selector.textContent).toContain(branch);
  expect(selector.textContent).not.toMatch(/[\u2066-\u2069]/u);
  expect(selector.querySelector("[data-inline-reference]")).toBeNull();
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

/** The uncommitted changes listed, and the comparison behind the Committed section failed: the failure that is a mark on a heading. */
const COMMITTED_FAILED: Partial<ChangesViewProps> = {
  uncommitted: {
    state: "listed",
    list: { state: "loaded", head: "c1", items: [], complete: true, refreshing: false },
    worktree: { id: "w", root: "/r", main: true, head: "c1", branch: "feature", scope_present: true },
    own: true,
  },
  committed: { state: "compare", branch: "feature", base: "main", baseKind: "remote", comparison: { state: "error", message: "Unable to read the base" }, commits: undefined },
};

const committedHeading = (container: HTMLElement) => [...container.querySelectorAll<HTMLElement>("[data-section-heading]")].find((heading) => heading.textContent?.startsWith("Committed"))!;

it("marks a part that could not be listed on the heading of its section, with no Alert and no standalone glyph", () => {
  const { container, onRetry } = mountPartFailure(COMMITTED_FAILED);
  expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
  const heading = committedHeading(container);
  const mark = heading.parentElement!.querySelector<HTMLButtonElement>("button[aria-label]:not([data-section-heading])")!;
  expect(mark.getAttribute("aria-label")).toBe("Could not compare this branch");
  press(mark);
  const dialog = document.querySelector('[role="dialog"]')!;
  expect(dialog.textContent).toContain("Unable to read the base");
  act(() => [...dialog.querySelectorAll("button")].find((button) => button.textContent === "Try again")!.click());
  expect(onRetry).toHaveBeenCalledOnce();
});

/** The heading's band and the failure mark it carries, in a view with one part failed. */
function headingWithMark(onRetry = vi.fn()) {
  const { container } = mountPartFailure(COMMITTED_FAILED, onRetry);
  const band = committedHeading(container);
  const mark = band.parentElement!.querySelector<HTMLButtonElement>("[data-section-heading-control]")!;
  const key = (target: HTMLElement, name: string) => act(() => void target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true })));
  // Focus put on a heading from outside the list is sent to the first row, as the list does for anything
  // that is no row; from a row the walk's Up reaches it, as it does for the keyboard.
  const focusBand = () => {
    const row = container.querySelector<HTMLElement>('[role="row"][aria-label]')!;
    act(() => row.focus());
    // The Committed heading has no rows, so the walk reaches it from the last row of the section above.
    key(row, "ArrowDown");
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
  const { container } = mountPartFailure(COMMITTED_FAILED);
  expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
  const headings = [...container.querySelectorAll("[data-section-heading]")].map((heading) => heading.textContent);
  expect(headings).toEqual(["Staged1", "Committed0"]);
  const controls = container.querySelectorAll("[data-section-heading-control]");
  expect(controls).toHaveLength(1);
  expect(committedHeading(container).parentElement!.contains(controls[0])).toBe(true);
});

it("shows an uncommitted listing failure with other changes as an Alert above the sections, with no mark and no alert role", () => {
  const { container, onRetry } = mountPartFailure({});
  const alert = [...container.querySelectorAll<HTMLElement>('[data-slot="alert-root"]')].find((element) => element.textContent?.includes("Could not list the changes in this worktree"))!;
  expect(alert).toBeDefined();
  expect(alert.textContent).toContain("Unable to read the index");
  // Above the list, not inside it, and not announced by itself: the view's status region speaks it.
  expect(alert.closest('[role="treegrid"], [role="grid"], [role="tree"]')).toBeNull();
  expect(alert.getAttribute("role")).not.toBe("alert");
  expect(alert.querySelector('[role="alert"]')).toBeNull();
  const list = container.querySelector('[role="treegrid"], [role="grid"], [role="tree"]')!;
  expect(alert.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(container.querySelector('button[aria-label="Could not list the changes in this worktree"]')).toBeNull();
  expect(container.querySelector("[data-section-heading-control]")).toBeNull();
  expect([...container.querySelectorAll("[data-section-heading]")].map((heading) => heading.textContent)).toEqual(["Staged1"]);
  act(() => [...alert.querySelectorAll("button")].find((button) => button.textContent === "Try again")!.click());
  expect(onRetry).toHaveBeenCalledOnce();
});

it("speaks the title of an uncommitted listing failure once through the status region when it appears after paint", async () => {
  const listed = COMMITTED_FAILED.uncommitted!;
  const failed: ChangesViewProps["uncommitted"] = { ...(listed as Extract<typeof listed, { state: "listed" }>), list: { state: "error", message: "Unable to read the index", unavailable: false, refreshing: false } };
  let fail = () => {};
  function Host() {
    const [uncommitted, setUncommitted] = useState(listed);
    fail = () => setUncommitted(failed);
    return (
      <GitView
        source={{ state: "loaded", source: source("/r", "/r"), sentAt: 0 }}
        branches={{ state: "loaded", branches: [{ name: "feature", commit: "c1" }], complete: true, sentAt: 0 }}
        branch="feature"
        onBranchChange={() => {}}
        uncommitted={uncommitted}
        committed={{ state: "none" }}
        items={[changeItem(change("staged", "readme.md"))]}
        selectedChange={undefined}
        onOpen={() => {}}
        onRetry={() => {}}
        listRef={() => {}}
        selectorsRef={{ current: null }}
        changeView={changeViewFixture()}
        selectedSessionId={undefined}
      />
    );
  }
  const container = mountRows(<Host />);
  const announcement = () => container.querySelector('[role="status"].sr-only')?.textContent ?? "";
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(announcement()).toBe("");
  act(() => fail());
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(announcement()).toBe("Could not list the changes in this worktree");
});

/** The text of the mark with `label` on a heading of the view, and whether the mark stands in the heading's own row. */
function openCutMark(container: HTMLElement, label: string): { text: string; heading: string } {
  const mark = container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
  expect(mark).not.toBeNull();
  const heading = mark.closest("[role=row]")!.querySelector("[data-section-heading]")!.textContent!;
  press(mark);
  const dialog = document.querySelector('[role="dialog"]')!;
  // The popover is named by the mark's label, which is not its first sentence.
  expect(dialog.getAttribute("aria-label")).toBe(label);
  const text = dialog.textContent ?? "";
  return { text, heading };
}

it("marks uncommitted changes cut short at the end of the worktree's line, naming the project's own working copy for it, with no Alert", () => {
  const { container } = mountPartFailure({
    uncommitted: {
      state: "listed",
      list: { state: "loaded", head: "c1", items: [], complete: false, refreshing: false },
      worktree: { id: "w", root: "/r", main: true, head: "c1", branch: "feature", scope_present: true },
      own: true,
    },
  });
  expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
  const marks = container.querySelectorAll<HTMLButtonElement>('button[aria-label="Not every uncommitted change is listed"]');
  expect(marks).toHaveLength(1);
  const line = marks[0].closest("p")!;
  expect(line.textContent).toContain("Worktree r");
  expect(line.lastElementChild).toBe(marks[0]);
  press(marks[0]);
  expect(document.querySelector('[role="dialog"]')!.textContent).toBe(
    "This worktree has more uncommitted changes than the list can show, often because generated files are not ignored."
      + "Add files that should not be tracked to .gitignore, or commit or discard some of the changes, then refresh.",
  );
});

it("marks the committed changes and the commits that were cut short on the Committed heading, in one popover", () => {
  const { container } = mountPartFailure({
    uncommitted: {
      state: "listed",
      list: { state: "loaded", head: "c1", items: [], complete: true, refreshing: false },
      worktree: { id: "w", root: "/r", main: true, head: "c1", branch: "feature", scope_present: true },
      own: true,
    },
    items: [changeItem(change("staged", "readme.md"))],
    committed: {
      state: "compare",
      branch: "feature",
      base: "main",
      baseKind: "local",
      comparison: { state: "loaded", left: { branch: "main", commit: "c1" }, right: { branch: "feature", commit: "c2" }, base: "c1000000000", items: [], complete: true, receivedAt: 0 },
      commits: { state: "loaded", base: "c1", tip: "c2", items: [], count: 100, complete: false, commitsComplete: false },
    },
  });
  const { text, heading } = openCutMark(container, "Committed changes are cut short");
  expect(heading).toContain("Committed");
  expect(text).toContain("Not every change is shown.");
  expect(text).toContain("Only the 100 newest commits are listed.");
  expect(container.querySelector("p.groove-t")).toBeNull();
});

/** A comparison of `feature` with `main` made at time 0, shown while the branch list is read again at time 1. */
function mountGoneComparison() {
  const both = [{ name: "main", commit: "c1" }, { name: "feature", commit: "c2" }];
  const list = (branches: typeof both, error?: DaemonRequestError): ChangesViewProps["branches"] => ({ state: "loaded", branches, complete: true, sentAt: 1, error });
  let read = (_branches: typeof both, _error?: DaemonRequestError) => {};
  const selectorsRef = { current: null as HTMLDivElement | null };
  function Host() {
    const [branches, setBranches] = useState(list(both));
    read = (next, error) => setBranches(list(next, error));
    return (
      <GitView
        source={{ state: "loaded", source: source("/r", "/r"), sentAt: 0 }}
        branches={branches}
        branch="feature"
        onBranchChange={() => {}}
        uncommitted={{ state: "notCheckedOut", branch: "feature" }}
        committed={{
          state: "compare",
          branch: "feature",
          base: "main",
          baseKind: "local",
          comparison: { state: "loaded", left: { branch: "main", commit: "c1" }, right: { branch: "feature", commit: "c2" }, base: "c1000000000", items: [], complete: true, receivedAt: 0 },
          commits: undefined,
        }}
        items={[changeItem(change("staged", "readme.md"))]}
        selectedChange={undefined}
        onOpen={() => {}}
        onRetry={() => {}}
        listRef={() => {}}
        selectorsRef={selectorsRef}
        changeView={changeViewFixture()}
        selectedSessionId={undefined}
      />
    );
  }
  const container = mountRows(<Host />);
  return { container, both, read: (next: typeof both, error?: DaemonRequestError) => act(() => read(next, error)), selectorsRef };
}

const GONE_MARK = 'button[aria-label$=" deleted"]';
const MAIN_GONE = "\u2068main\u2069 deleted";

it("marks a deleted base branch at its name in the Forked-from line, with no loose text, Alert or status line, and says it once when found gone", async () => {
  const { container, both, read } = mountGoneComparison();
  expect(container.querySelector(GONE_MARK)).toBeNull();
  read([both[1]]);
  const mark = container.querySelector<HTMLButtonElement>(GONE_MARK)!;
  expect(mark).not.toBeNull();
  const line = mark.closest("p")!;
  expect(line.textContent).toContain("Forked from main");
  expect(mark.getAttribute("aria-label")).toBe(MAIN_GONE);
  // Right after the base's name, before the commit it forked at.
  const [baseName, baseCommit] = line.querySelectorAll("[data-inline-reference]");
  expect(baseName.compareDocumentPosition(mark) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(baseCommit.compareDocumentPosition(mark) & Node.DOCUMENT_POSITION_PRECEDING).toBeTruthy();
  expect(container.textContent).not.toContain("no longer exists");
  expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(container.querySelectorAll('[role="status"]:not(.sr-only)')).toHaveLength(0);
  press(mark);
  expect(document.querySelector('[role="dialog"]')!.textContent).toBe("main no longer exists. The comparison still uses the commits it was made at.");
  await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(container.querySelector('[role="status"].sr-only')!.textContent).toBe(MAIN_GONE);
});

it("leaves a comparison whose branches are all listed unmarked, and gives focus on the mark to the branch selector when it goes", () => {
  const { container, both, read, selectorsRef } = mountGoneComparison();
  read([both[1]]);
  const mark = container.querySelector<HTMLButtonElement>(GONE_MARK)!;
  act(() => mark.focus());
  expect(document.activeElement).toBe(mark);
  read(both);
  expect(mark.isConnected).toBe(false);
  expect(container.querySelector(GONE_MARK)).toBeNull();
  expect(document.activeElement).toBe(selectorsRef.current!.querySelector("button"));
});

it("puts no mark beside the Branch select for a compared branch that is gone, the select's own badge saying so", () => {
  const { container, both, read, selectorsRef } = mountGoneComparison();
  read([both[0]]);
  expect(container.querySelector(GONE_MARK)).toBeNull();
  expect(selectorsRef.current!.querySelector("[data-status-mark]")).toBeNull();
  expect(selectorsRef.current!.textContent).toContain("No longer exists");
});
