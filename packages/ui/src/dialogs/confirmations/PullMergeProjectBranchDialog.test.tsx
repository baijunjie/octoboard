// @vitest-environment jsdom
import { act, useState } from "react";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import { withModality } from "../../components/dialog/dialogTestSupport";
import { MODAL_CLOSE_SETTLE_MS, setFocusAfterModalLoss } from "../../components/focus/modalFocus";
import { CodedError } from "../../errors/codedError";
import { projectOf, snapshotState } from "../../fixtures/builders";
import type { RequestHandler } from "../../requests";
import { ErrorAlerts } from "../../shell/ErrorAlerts";
import { acknowledgeAllAlerts } from "../../test/alertQueue";
import { fakeTimers, runOutTimers } from "../../test/clock";
import { mountHost } from "../../test/mountHost";
import { testDaemon } from "../../test/testDaemon";
import { PullMergeProjectBranchDialog } from "./PullMergeProjectBranchDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(acknowledgeAllAlerts);

const website = projectOf("p", "c", "Website");

function mountDialog(request: RequestHandler, counts: { ahead: number; behind: number } = { ahead: 2, behind: 3 }) {
  // A previous test's script click leaves virtual modality, which defers react-aria's opening focus to a frame.
  withModality("pointer");
  fakeTimers();
  const onClose = vi.fn();
  const showNotice = vi.fn();
  const daemon = testDaemon({ request, showNotice, state: snapshotState({ projects: [website] }) });
  mountHost(
    <>
      <PullMergeProjectBranchDialog project={website} ahead={counts.ahead} behind={counts.behind} onClose={onClose} />
      <ErrorAlerts />
    </>,
    { daemon },
  );
  return { onClose, showNotice, daemon };
}

const confirmation = () => document.querySelector('[role="alertdialog"]')!;
const confirm = () => act(async () => confirmation().querySelector("form")!.requestSubmit());
const pulled = (outcome: object) => vi.fn<RequestHandler>(async () => ({ type: "project_branch_pulled", id: null, outcome }) as never);

it("asks for the pull and merge of the named project, stating the counts and the merge commit", () => {
  mountDialog(pulled({ kind: "merged" }));
  const dialog = confirmation();
  expect(dialog.querySelector("h2")?.textContent).toBe("Pull and merge");
  expect(dialog.textContent).toContain("Website");
  expect(dialog.textContent).toContain("has diverged from its upstream: 2 commits ahead and 3 commits behind.");
  expect(dialog.textContent).toContain("creates a merge commit");
  expect(dialog.querySelector("button[type=submit]")?.textContent).toBe("Pull and merge");
  expect(dialog.textContent).toContain("Cancel");
});

it.each([
  [1, 1, "1 commit ahead and 1 commit behind."],
  [1, 3, "1 commit ahead and 3 commits behind."],
  [2, 1, "2 commits ahead and 1 commit behind."],
  [12, 30, "12 commits ahead and 30 commits behind."],
])("words %i commits ahead and %i behind in its own plural forms: %s", (ahead, behind, counts) => {
  mountDialog(pulled({ kind: "merged" }), { ahead, behind });
  expect(confirmation().textContent).toContain(`has diverged from its upstream: ${counts}`);
});

it("starts on the dialog itself rather than on Pull and merge, so an Enter typed elsewhere confirms nothing", () => {
  const request = pulled({ kind: "merged" });
  mountDialog(request);
  runOutTimers();
  const active = document.activeElement!;
  expect(active).toBe(confirmation());
  expect(active.matches("button")).toBe(false);
  act(() => {
    active.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    active.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true, cancelable: true }));
  });
  expect(request).not.toHaveBeenCalled();
});

it("sends nothing until confirmed, and closing it sends nothing at all", async () => {
  const request = pulled({ kind: "merged" });
  const { onClose, showNotice } = mountDialog(request);
  expect(request).not.toHaveBeenCalled();
  await act(async () => {
    [...confirmation().querySelectorAll("button")].find((button) => button.textContent === "Cancel")!.click();
  });
  expect(onClose).toHaveBeenCalledOnce();
  expect(request).not.toHaveBeenCalled();
  expect(showNotice).not.toHaveBeenCalled();
});

it.each([
  [{ kind: "merged" }],
  [{ kind: "fast_forwarded" }],
])("asks the daemon to merge the project and, once it ended %j, says the branch was pulled and closes", async (outcome) => {
  const request = pulled(outcome);
  const { onClose, showNotice } = mountDialog(request);
  await confirm();
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "pull_project_branch", project: "p", merge: true });
  expect(showNotice).toHaveBeenCalledExactlyOnceWith("Pulled the current branch.");
  expect(onClose).toHaveBeenCalledOnce();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("says the branch is up to date when the confirmed pull finds nothing to pull any more", async () => {
  const { onClose, showNotice } = mountDialog(pulled({ kind: "up_to_date" }));
  await confirm();
  expect(showNotice).toHaveBeenCalledExactlyOnceWith("The current branch is already up to date.");
  expect(onClose).toHaveBeenCalledOnce();
});

it("turns into a failure modal for the pull, with the conflicted paths under Error details, when the merge met conflicts", async () => {
  const detail = "src/a.ts\nsrc/b.ts";
  const { onClose, showNotice } = mountDialog(async () => {
    throw new CodedError(`Merging the upstream into this branch causes conflicts, so the merge was cancelled and nothing changed: ${detail}`, "pull_merge_conflict", { detail });
  });
  await confirm();
  expect(onClose).not.toHaveBeenCalled();
  expect(showNotice).not.toHaveBeenCalled();
  // The dialog itself says it, with no alert raised over a question that has nothing left to offer.
  expect(errorAlerts.getSnapshot()).toEqual([]);
  const dialogs = [...document.querySelectorAll('[role="alertdialog"]')];
  expect(dialogs).toHaveLength(1);
  expect(dialogs[0].textContent).toContain("Could not pull the branch");
  expect(dialogs[0].textContent).toContain("the merge was cancelled and nothing changed");
  expect(dialogs[0].querySelector("pre")?.textContent).toBe(detail);
  expect([...dialogs[0].querySelectorAll("button[type=submit]")].map((button) => button.textContent)).not.toContain("Pull and merge");
});

it("offers only an acknowledgement once the merge met conflicts, which closes the dialog without pulling again", async () => {
  const request = vi.fn<RequestHandler>(async () => {
    throw new CodedError("Merging the upstream into this branch causes conflicts, so the merge was cancelled and nothing changed: src/a.ts", "pull_merge_conflict", { detail: "src/a.ts" });
  });
  const { onClose, showNotice } = mountDialog(request);
  await confirm();
  const dialog = confirmation();
  const buttons = [...dialog.querySelectorAll("button")].map((button) => button.textContent);
  expect(buttons).not.toContain("Pull and merge");
  expect(buttons).not.toContain("Cancel");
  expect(buttons).toContain("OK");
  expect(dialog.textContent).toContain("Error details");
  await act(async () => [...dialog.querySelectorAll("button")].find((button) => button.textContent === "OK")!.click());
  expect(onClose).toHaveBeenCalledOnce();
  expect(request).toHaveBeenCalledOnce();
  expect(showNotice).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("keeps Pull and merge on offer under the alert for any other refusal, so the pull can be tried again", async () => {
  mountDialog(async () => {
    throw new CodedError("Git could not pull this branch: refused by the hook", "git_pull_failed", { detail: "refused by the hook" });
  });
  await confirm();
  expect(confirmation().textContent).toContain("Pull and merge");
  expect(confirmation().querySelector("button[type=submit]")?.textContent).toBe("Pull and merge");
});

it("raises an alert for any other refusal of the confirmed pull, and says nothing was pulled", async () => {
  const { onClose, showNotice } = mountDialog(async () => {
    throw new CodedError("Git could not pull this branch: refused by the hook", "git_pull_failed", { detail: "refused by the hook" });
  });
  await confirm();
  expect(onClose).not.toHaveBeenCalled();
  expect(showNotice).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ title: "Could not pull the branch", error: expect.objectContaining({ code: "git_pull_failed" }) })]);
});

it("treats a reply that asks for a merge to a request that already confirmed one as an unexpected failure", async () => {
  const { onClose, showNotice } = mountDialog(pulled({ kind: "needs_merge", ahead: 1, behind: 1 }));
  await confirm();
  expect(onClose).not.toHaveBeenCalled();
  expect(showNotice).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ title: "Could not pull the branch" })]);
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
});

it("closes without a word, sending nothing, when the project is removed while the confirmation is open", async () => {
  const request = pulled({ kind: "merged" });
  const { onClose, showNotice, daemon } = mountDialog(request);
  expect(onClose).not.toHaveBeenCalled();
  await act(async () => daemon.store.setState({ projects: new Map() }));
  expect(onClose).toHaveBeenCalledOnce();
  expect(request).not.toHaveBeenCalled();
  expect(showNotice).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

// A confirmation for other figures or another project is a new question on the same mounted dialog, which must not show
// the failure the one before it turned into.
it.each([
  ["the same project's other counts", { project: website, ahead: 4, behind: 5 }],
  ["another project", { project: projectOf("p-2", "c", "API"), ahead: 2, behind: 3 }],
])("asks the question afresh on the same dialog for %s, not showing the conflict of the one before", async (_case, next) => {
  withModality("pointer");
  fakeTimers();
  const conflicts = (detail: string) => new CodedError(`Merging the upstream into this branch causes conflicts, so the merge was cancelled and nothing changed: ${detail}`, "pull_merge_conflict", { detail });
  const request = vi.fn<RequestHandler>(async (body) => {
    throw conflicts((body as { project: string }).project === website.id ? "src/a.ts" : "src/other.ts");
  });
  const daemon = testDaemon({ request, state: snapshotState({ projects: [website, next.project] }) });
  const showing = (subject: { project: typeof website; ahead: number; behind: number }) => (
    <PullMergeProjectBranchDialog project={subject.project} ahead={subject.ahead} behind={subject.behind} onClose={() => {}} />
  );
  const host = mountHost(showing({ project: website, ahead: 2, behind: 3 }), { daemon });
  await confirm();
  expect(confirmation().querySelector("pre")?.textContent).toBe("src/a.ts");
  expect(confirmation().textContent).not.toContain("Pull and merge");
  const before = confirmation();

  host.render(showing(next));
  expect(confirmation()).toBe(before);
  expect(confirmation().querySelector("pre")).toBeNull();
  expect(confirmation().querySelector("button[type=submit]")?.textContent).toBe("Pull and merge");
  expect(confirmation().textContent).toContain(`${next.ahead} commits ahead and ${next.behind} commits behind`);
  expect(confirmation().textContent).toContain(next.project.name);

  // The new question is asked on its own: confirming it pulls the new project, and its conflict is the one shown.
  request.mockClear();
  await confirm();
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "pull_project_branch", project: next.project.id, merge: true });
  expect(confirmation().querySelector("pre")?.textContent).toBe(next.project.id === website.id ? "src/a.ts" : "src/other.ts");
});

it("hands focus on to the window's fallback when the project's removal closes the confirmation and leaves focus on the page body", async () => {
  withModality("pointer");
  fakeTimers();
  const fallback = vi.fn();
  onTestFinished(setFocusAfterModalLoss(fallback));
  const daemon = testDaemon({ state: snapshotState({ projects: [website] }) });
  function Closing() {
    const [open, setOpen] = useState(true);
    return open ? <PullMergeProjectBranchDialog project={website} ahead={2} behind={3} onClose={() => setOpen(false)} /> : null;
  }
  mountHost(<Closing />, { daemon });
  expect(confirmation()).not.toBeNull();
  await act(async () => daemon.store.setState({ projects: new Map() }));
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  (document.activeElement as HTMLElement | null)?.blur();
  expect(fallback).not.toHaveBeenCalled();
  await act(async () => { vi.advanceTimersByTime(MODAL_CLOSE_SETTLE_MS); });
  expect(fallback).toHaveBeenCalledOnce();
});
