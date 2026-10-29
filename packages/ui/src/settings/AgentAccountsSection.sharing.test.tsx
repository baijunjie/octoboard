// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import type { AccountSharing } from "../protocol";
import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { currentLanguage, setLanguage } from "../i18n/language";
import { press } from "../components/tooltipTestSupport";
import { accountSharing, request, settings } from "./agentAccountsSectionTestState";
import { buttonNamed, mount, mountSettings, openSettingsOf, sharingSwitches } from "./agentAccountsSectionTestSupport";

vi.mock("../store", async (importOriginal) => (await import("./agentAccountsSectionTestState")).storeStandIn(await importOriginal()));
vi.mock("../components/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../components/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../components/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("./useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../dialogs/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
beforeEach(() => report.mockClear());
/** The title and the daemon code of each error alert raised, in order. */
const reported = () => report.mock.calls.map(([alert]) => [alert.title, (alert.error as { code?: string }).code]);

/** The text of the notes that describe `sharing`, as a screen reader reads them. */
const captionOf = (container: HTMLElement, sharing: HTMLElement) =>
  (sharing.getAttribute("aria-describedby") ?? "").split(" ").filter(Boolean).map((id) => container.querySelector(`[id="${id}"]`)?.textContent).join(" ");

/** The text of the sharing help tip, opened by keyboard focus. */
function helpTipText(container: HTMLElement): string | undefined {
  const tip = container.querySelector<HTMLButtonElement>('button[aria-label="About sharing the default configuration"]')!;
  press(tip);
  return document.querySelector('[role="dialog"][aria-label="About sharing the default configuration"]')?.textContent ?? undefined;
}

it("offers the sharing switch on each owned account and none on the default account", async () => {
  const { container } = await mountSettings("Codex");
  const switches = sharingSwitches(container);
  expect(switches).toHaveLength(1);
  expect(switches[0]!.getAttribute("aria-label")?.replace(/[\u2068\u2069]/g, "")).toBe("Share default configuration for codex-work");
  expect(switches[0]!.checked).toBe(false);
  expect(switches[0]!.hasAttribute("aria-disabled")).toBe(false);
  // Free, nothing stands under the switch; what turning it on does is its description, and the rest is in the help tip.
  expect(captionOf(container, switches[0]!)).toBe("");
  expect(container.textContent).toContain("Turning this on links the account’s configuration and conversation history to the default account’s directory.");
  expect(container.textContent).not.toContain("Applies at once");
  expect(container.textContent).not.toContain("has not been applied to the account’s directory yet");
  expect(container.querySelector('[role="progressbar"]')).toBeNull();
});

it("says what is the case now when the account shares", async () => {
  settings.accounts[1]!.share_default_config = true;
  const { container } = await mountSettings("Codex");
  const [sharing] = sharingSwitches(container);
  expect(sharing!.checked).toBe(true);
  expect(captionOf(container, sharing!)).toBe("");
  expect(container.textContent).toContain("The account’s configuration and conversation history are linked to the default account’s directory.");
});

it.each([["Claude Code", [".claude.json", "plans/"]], ["Grok Build", ["trusted_folders.toml", "hooks-paths"]]])(
  "keeps each name in the %s sharing help tip in one box, so a line never ends on an empty start of it",
  async (tab, names) => {
    const { container } = await mountSettings(tab);
    const tip = container.querySelector<HTMLButtonElement>('button[aria-label="About sharing the default configuration"]')!;
    press(tip);
    const chips = [...document.querySelectorAll('[role="dialog"] .inline-reference')].filter((chip) => chip.querySelector("code"));
    expect(chips.map((chip) => chip.textContent)).toEqual(names);
    // Unbroken, so a line never ends on an empty start of it; the chip's own stylesheet tightens the inline-block
    // so stacked chips keep the line gap.
    for (const chip of chips) expect(chip.classList).toContain("inline-reference--unbroken");
  },
);

describe("a switch that cannot be changed now", () => {
  it.each([
    [1, "Sharing cannot be changed while 1 of this account’s sessions is working. Wait for it to finish."],
    [3, "Sharing cannot be changed while 3 of this account’s sessions are working. Wait for them to finish."],
  ])("is disabled and says how many sessions run (%i)", async (sessions, text) => {
    accountSharing.set("codex-work", { account: "codex-work", in_use: { sessions, signing_in: false } });
    const { container } = await mountSettings("Codex");
    const [sharing] = sharingSwitches(container);
    expect(sharing!.getAttribute("aria-disabled")).toBe("true");
    expect(captionOf(container, sharing!)).toBe(text);
  });

  it("is disabled and says the account is being signed in", async () => {
    accountSharing.set("codex-work", { account: "codex-work", in_use: { sessions: 0, signing_in: true } });
    const { container } = await mountSettings("Codex");
    const [sharing] = sharingSwitches(container);
    expect(sharing!.getAttribute("aria-disabled")).toBe("true");
    expect(captionOf(container, sharing!)).toBe("Sharing cannot be changed while this account is being signed in.");
  });

  it("says both reasons when sessions run and the account is being signed in", async () => {
    accountSharing.set("codex-work", { account: "codex-work", in_use: { sessions: 2, signing_in: true } });
    const { container } = await mountSettings("Codex");
    const text = captionOf(container, sharingSwitches(container)[0]!);
    expect(text).toContain("2 of this account’s sessions are working");
    expect(text).toContain("being signed in");
  });

  it("is free again, with nothing under it, once nothing keeps it", async () => {
    accountSharing.set("codex-work", { account: "codex-work", in_use: { sessions: 0, signing_in: false } });
    const { container } = await mountSettings("Codex");
    expect(sharingSwitches(container)[0]!.hasAttribute("aria-disabled")).toBe(false);
    expect(captionOf(container, sharingSwitches(container)[0]!)).toBe("");
  });
});

/** The warning mark after the switch, which holds everything of an unfinished change. */
const mark = (container: HTMLElement) => container.querySelector<HTMLElement>("button[aria-label='Last sharing change']");
/** Opens the mark's popover and returns it. */
const openMark = (container: HTMLElement): HTMLElement => {
  press(mark(container)!);
  return document.querySelector<HTMLElement>("[data-status-mark-popover]")!;
};
/** The popover's paragraphs: the title, the reason (for a failure), the remedy, then the later guidance and the note. */
const paragraphs = (popover: HTMLElement) => [...popover.querySelectorAll("p")].map((paragraph) => paragraph.textContent);

describe("idle sessions around a change", () => {
  it.each([
    [1, "Changing sharing ends this account’s 1 idle session and resumes it afterwards."],
    [4, "Changing sharing ends this account’s 4 idle sessions and resumes them afterwards."],
  ])("keeps the switch free and says in the help tip the %i idle sessions will be restarted", async (idle_sessions, text) => {
    accountSharing.set("codex-work", { account: "codex-work", idle_sessions });
    const { container } = await mountSettings("Codex");
    const [sharing] = sharingSwitches(container);
    expect(sharing!.hasAttribute("aria-disabled")).toBe(false);
    expect(captionOf(container, sharing!)).toBe("");
    expect(helpTipText(container)).toContain(text);
  });

  it("says the idle sessions are restarted by a retry too, in the unfinished warning's popover", async () => {
    settings.accounts[1]!.share_default_config = true;
    settings.accounts[1]!.share_default_config_incomplete = true;
    accountSharing.set("codex-work", { account: "codex-work", idle_sessions: 2 });
    const { container } = await mountSettings("Codex");
    expect(paragraphs(openMark(container))).toEqual([
      "The last attempt to turn sharing on did not finish.",
      "Turn the switch on again to try once more.",
      "Changing sharing ends this account’s 2 idle sessions and resumes them afterwards.",
    ]);
  });

  it("says nothing of idle sessions when there are none, and puts working sessions first when both exist", async () => {
    accountSharing.set("codex-work", { account: "codex-work", idle_sessions: 2, in_use: { sessions: 1, signing_in: false } });
    const { container } = await mountSettings("Codex");
    const text = captionOf(container, sharingSwitches(container)[0]!);
    expect(text).toBe("Sharing cannot be changed while 1 of this account’s sessions is working. Wait for it to finish.");
  });
});

describe("the phases of a running change", () => {
  const phase = (progress: NonNullable<AccountSharing["progress"]>) => accountSharing.set("codex-work", { account: "codex-work", progress });
  const bar = (container: HTMLElement) => container.querySelector('[role="progressbar"]')!;
  const valueText = (container: HTMLElement) => container.querySelector(`[id="${bar(container).getAttribute("aria-describedby")}"]`)?.textContent;

  it.each([
    ["waiting", "Waiting for another account’s sharing change to finish…"],
    ["stopping", "Ending idle sessions…"],
    ["resuming", "Resuming sessions…"],
  ] as const)("shows %s as an indeterminate bar that says what it waits for", async (name, text) => {
    phase({ share: true, phase: name, done: name === "resuming" ? 5 : 0, total: 5 });
    const { container } = await mountSettings("Codex");
    expect(bar(container).hasAttribute("aria-valuenow")).toBe(false);
    expect(valueText(container)).toBe(text);
    expect(container.textContent).not.toContain("Now:");
    expect(sharingSwitches(container)[0]!.getAttribute("aria-disabled")).toBe("true");
  });

  it("counts entries only while changing, and waits to start until the first one", async () => {
    phase({ share: true, phase: "changing", done: 0, total: 5 });
    const { container, render } = await mountSettings("Codex");
    expect(valueText(container)).toBe("Waiting to start…");
    phase({ share: true, phase: "changing", done: 2, total: 5, item: "skills" });
    render();
    expect(bar(container).getAttribute("aria-valuenow")).toBe("2");
    expect(container.textContent).toContain("2 of 5");
    expect(container.textContent).toContain("Now: skills");
  });
});

describe("the announcement of the phases", () => {
  it("keeps what was said when the entries phase starts, and speaks again for the other phases", async () => {
    const set = (phase: "waiting" | "stopping" | "changing" | "resuming") => accountSharing.set("codex-work", { account: "codex-work", progress: { share: true, phase, done: 0, total: 3 } });
    vi.useFakeTimers();
    onTestFinished(() => { vi.useRealTimers(); });
    set("stopping");
    const { container, render } = await mountSettings("Codex");
    act(() => { vi.advanceTimersByTime(300); });
    const said = () => [...container.querySelectorAll('[role="status"]')].map((node) => node.textContent).find((text) => text?.startsWith("Turning sharing on"));
    expect(said()).toBe("Turning sharing on. Ending idle sessions…");
    set("changing");
    render();
    act(() => { vi.advanceTimersByTime(300); });
    expect(said()).toBe("Turning sharing on. Ending idle sessions…");
    set("resuming");
    render();
    expect(said()).toBe("Turning sharing on. Resuming sessions…");
  });

  it("joins the direction and the phase with the Chinese full stop in Chinese", async () => {
    const before = currentLanguage();
    setLanguage("zh-Hans");
    onTestFinished(() => { act(() => setLanguage(before)); });
    accountSharing.set("codex-work", { account: "codex-work", progress: { share: true, phase: "stopping", done: 0, total: 3 } });
    vi.useFakeTimers();
    onTestFinished(() => { vi.useRealTimers(); });
    const { container: section } = mount("Codex");
    const container = await openSettingsOf(section, "codex-work", "设置");
    act(() => { vi.advanceTimersByTime(300); });
    const said = [...container.querySelectorAll('[role="status"]')].map((node) => node.textContent).find((text) => text?.startsWith("正在开启共享"));
    expect(said).toBe("正在开启共享。正在结束空闲会话…");
    expect(said).not.toContain(". ");
  });

  it("shows the progress and not the warning when an old failure is still held during the wait", async () => {
    settings.accounts[1]!.share_default_config_incomplete = true;
    accountSharing.set("codex-work", { account: "codex-work", failure: { share: true, items: [], detail: "x" }, progress: { share: true, phase: "waiting", done: 0, total: 3 } });
    const { container } = await mountSettings("Codex");
    expect(mark(container)).toBeNull();
    expect(container.querySelector('[role="progressbar"]')).not.toBeNull();
  });
});

describe("a change that runs", () => {
  const running = (progress: NonNullable<AccountSharing["progress"]>) => accountSharing.set("codex-work", { account: "codex-work", progress });

  it("disables the switch under a determinate progress bar with the count and the entry", async () => {
    settings.accounts[1]!.share_default_config = true;
    running({ share: true, phase: "changing", done: 3, total: 12, item: "skills" });
    const { container } = await mountSettings("Codex");
    const [sharing] = sharingSwitches(container);
    expect(sharing!.getAttribute("aria-disabled")).toBe("true");
    expect(sharing!.checked).toBe(true);
    const bar = container.querySelector('[role="progressbar"]')!;
    expect(bar.hasAttribute("aria-label")).toBe(false);
    expect(container.querySelector(`[id="${bar.getAttribute("aria-labelledby")}"]`)?.textContent).toBe("Turning sharing on");
    expect(bar.getAttribute("aria-valuenow")).toBe("3");
    expect(bar.getAttribute("aria-valuemin")).toBe("0");
    expect(bar.getAttribute("aria-valuemax")).toBe("12");
    expect(bar.getAttribute("aria-valuetext")).toBe("3 of 12");
    expect(bar.textContent).toContain("Turning sharing on");
    expect(container.textContent).toContain("Now: skills");
    const chip = container.querySelector('[id$="-current"] .inline-reference')!;
    expect(chip.classList).toContain("inline-reference--unbroken");
    expect(container.textContent).not.toContain("Turning this on links");
    expect(captionOf(container, sharing!)).toBe("Turning sharing on 3 of 12 Now: skills");
  });

  it("words the direction of turning sharing off", async () => {
    running({ share: false, phase: "changing", done: 0, total: 4, item: "019e3c2a-0000-7000-8000-000000000000" });
    const { container } = await mountSettings("Codex");
    const bar = container.querySelector('[role="progressbar"]')!;
    expect(container.querySelector(`[id="${bar.getAttribute("aria-labelledby")}"]`)?.textContent).toBe("Turning sharing off");
    expect(bar.getAttribute("aria-valuenow")).toBe("0");
    expect(container.textContent).toContain("019e3c2a-0000-7000-8000-000000000000");
  });

  it("is indeterminate while it waits for short background reads", async () => {
    running({ share: true, phase: "changing", done: 0, total: 5 });
    const { container } = await mountSettings("Codex");
    const bar = container.querySelector('[role="progressbar"]')!;
    expect(bar.hasAttribute("aria-valuenow")).toBe(false);
    expect(captionOf(container, sharingSwitches(container)[0]!)).toBe("Turning sharing on Waiting to start…");
    expect(container.textContent).not.toContain("Now:");
    expect(sharingSwitches(container)[0]!.getAttribute("aria-disabled")).toBe("true");
  });

  it("follows the progress as the daemon reports it", async () => {
    running({ share: true, phase: "changing", done: 1, total: 4, item: "skills" });
    const { container, render } = await mountSettings("Codex");
    expect(container.querySelector('[role="progressbar"]')!.getAttribute("aria-valuenow")).toBe("1");
    running({ share: true, phase: "changing", done: 2, total: 4, item: "plugins" });
    render();
    expect(container.querySelector('[role="progressbar"]')!.getAttribute("aria-valuenow")).toBe("2");
    expect(container.textContent).toContain("Now: plugins");
    accountSharing.delete("codex-work");
    render();
    expect(container.querySelector('[role="progressbar"]')).toBeNull();
    expect(sharingSwitches(container)[0]!.hasAttribute("aria-disabled")).toBe(false);
  });
});

it("asks for sharing to be turned on and off for the account", async () => {
  const { container, render } = await mountSettings("Codex");
  act(() => sharingSwitches(container)[0]!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_account_sharing", account: "codex-work", share: true });
  await act(async () => {});
  settings.accounts[1]!.share_default_config = true;
  render();
  request.mockClear();
  act(() => sharingSwitches(container)[0]!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_account_sharing", account: "codex-work", share: false });
  await act(async () => {});
});

it("reports a refused sharing change and leaves the switch as the daemon records it", async () => {
  const { container } = await mountSettings("Codex");
  request.mockRejectedValueOnce(new DaemonRequestError("refused", "account_in_use", { account: "codex-work", name: "codex-work", sessions: "1", signing_in: "false" }));
  act(() => sharingSwitches(container)[0]!.click());
  await act(async () => {});
  expect(reported()).toEqual([["Could not change sharing", "account_in_use"]]);
  expect(sharingSwitches(container)[0]!.checked).toBe(false);
});

it("opens the error alert dialog when the change it asked for ends unfinished", async () => {
  const { container } = await mountSettings("Codex");
  request.mockRejectedValueOnce(new DaemonRequestError("failed", "account_sharing_failed", { account: "codex-work", name: "codex-work", items: "[]", detail: "x" }));
  act(() => sharingSwitches(container)[0]!.click());
  await act(async () => {});
  expect(reported()).toEqual([["Could not change sharing", "account_sharing_failed"]]);
});

describe("a change that did not finish", () => {
  const failure = (patch: Partial<NonNullable<AccountSharing["failure"]>> = {}): NonNullable<AccountSharing["failure"]> => ({
    share: true,
    cause: "permission_denied",
    items: [{ item: "skills/", left: "unchanged", detail: "permission denied", cause: "permission_denied" }],
    detail: "could not link skills",
    ...patch,
  });
  /** An account that records sharing on and whose last change to it did not finish. */
  const unfinished = (share = true) => {
    settings.accounts[1]!.share_default_config = share;
    settings.accounts[1]!.share_default_config_incomplete = true;
  };
  /** What the row's status region says once it has had its first write delay (`StatusAnnouncer`). */
  const spoken = async (container: HTMLElement) => {
    await act(async () => new Promise((resolve) => setTimeout(resolve, 150)));
    return [...container.querySelectorAll("[role=status]")].map((node) => node.textContent).join("");
  };

  it("shows a warning mark right after the switch, with the title, reason, remedy and raw entries in its popover, and the switch where it was before", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure() });
    const { container } = await mountSettings("Codex");
    const [sharing] = sharingSwitches(container);
    expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
    expect(mark(container)).not.toBeNull();
    expect(sharing!.compareDocumentPosition(mark(container)!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // Nothing of it is written beside the mark.
    const written = [...container.querySelectorAll("*")].filter((node) => node.children.length === 0 && node.textContent?.includes("Could not finish"));
    expect(written.every((node) => node.classList.contains("sr-only"))).toBe(true);
    const popover = openMark(container);
    // One cause: its own line and its fix, then the way to try again where the fix did not take its place.
    expect(paragraphs(popover)).toEqual([
      "Could not finish turning sharing on.",
      "You do not have permission to change skills/.",
      "Check that the files and folders involved can be changed and are not locked.",
      "Turn the switch on again to try once more.",
    ]);
    expect(popover.textContent).toContain("skills/ was left unchanged.permission denied");
    expect(popover.querySelector("li")?.lastElementChild?.textContent).toBe("permission denied");
    // Raw text is always a block of its own, never a chip in the sentence.
    expect(popover.querySelector("li pre")?.textContent).toBe("permission denied");
    // The switch is the retry, so there is no button for it.
    expect(buttonNamed(container, "Try again")).toBeUndefined();
    expect(sharing!.checked).toBe(false);
    // The switch is described by a hidden copy of the title, not by what is in the closed popover.
    expect(container.textContent).not.toContain("Turning this on links");
    expect(captionOf(container, sharing!)).toBe("Could not finish turning sharing on.");
  });

  it("asks for the recorded choice when the switch is turned, which is the retry", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure() });
    const { container } = await mountSettings("Codex");
    await act(async () => sharingSwitches(container)[0]!.click());
    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_account_sharing", account: "codex-work", share: true });
  });

  it("retries turning sharing off the same way, in the closing wording", async () => {
    unfinished(false);
    accountSharing.set("codex-work", { account: "codex-work", failure: failure({ share: false }) });
    const { container } = await mountSettings("Codex");
    const popover = openMark(container);
    expect(paragraphs(popover)[0]).toBe("Could not finish turning sharing off.");
    expect(popover.textContent).toContain("Turn the switch off again to try once more.");
    expect(sharingSwitches(container)[0]!.checked).toBe(true);
    await act(async () => sharingSwitches(container)[0]!.click());
    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_account_sharing", account: "codex-work", share: false });
  });

  it("groups the entries by cause, one line each, naming the first and how many more", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure({ cause: "other", items: [
      { item: "skills/", left: "unchanged", detail: "a", cause: "permission_denied" },
      { item: "plugins/", left: "unchanged", detail: "b", cause: "permission_denied" },
      { item: "agents/", left: "unchanged", detail: "c", cause: "permission_denied" },
      { item: "settings.json", left: "linked", detail: "d", cause: "busy" },
      { item: "hooks/", left: "unchanged", detail: "e" },
    ] }) });
    const { container } = await mountSettings("Codex");
    const popover = openMark(container);
    // Several causes: that some could not be changed and the way to try again; each cause follows.
    expect(paragraphs(popover)).toEqual([
      "Could not finish turning sharing on.",
      "Some files or folders could not be changed.",
      "Turn the switch on again to try once more.",
    ]);
    const lines = [...popover.querySelectorAll("span.flex.flex-col > span")].map((line) => line.textContent);
    expect(lines).toEqual([
      "You do not have permission to change skills/ and 2 more. Check that the files and folders involved can be changed and are not locked.",
      "settings.json is in use. Close other programs using the files involved, such as an agent in a terminal, an editor or a sync tool.",
      "Could not change hooks/. The system's own reason is below.",
    ]);
  });

  const retry = "Turn the switch on again to try once more.";
  it.each([
    ["permission_denied", "You do not have permission to change skills/.", "Check that the files and folders involved can be changed and are not locked."],
    ["busy", "skills/ is in use.", "Close other programs using the files involved, such as an agent in a terminal, an editor or a sync tool."],
    ["no_space", "There is not enough disk space to change skills/.", "Free up disk space."],
    ["conflict", "skills/ conflicts with what is already there.", "Move or rename the conflicting items."],
    ["not_found", "skills/ could not be found.", retry],
    ["other", "Could not change skills/.", "The system's own reason is below."],
  ] as const)("says a single %s cause as its own line and its fix, with the way to try again after them where the fix did not take its place", async (cause, line, fix) => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure({ cause, items: [{ item: "skills/", left: "unchanged", detail: "x", cause }] }) });
    const { container } = await mountSettings("Codex");
    const popover = openMark(container);
    expect(paragraphs(popover)).toEqual(["Could not finish turning sharing on.", line, fix, ...(fix === retry ? [] : [retry])]);
    // The line is not said again as a cause summary.
    expect(popover.querySelector("span.flex.flex-col")).toBeNull();
  });

  it.each([
    ["permission_denied", "You do not have permission to change the account’s directory.", "Check that the files and folders involved can be changed and are not locked."],
    ["busy", "Something in the account’s directory is in use.", "Close other programs using the files involved, such as an agent in a terminal, an editor or a sync tool."],
    ["no_space", "There is not enough disk space.", "Free up disk space."],
    ["conflict", "The account’s directory conflicts with what sharing needs.", "Move or rename the conflicting items."],
    ["not_found", "A directory needed for sharing could not be found.", retry],
  ] as const)("says a %s cause of a change that failed as a whole as its own line and its fix", async (cause, line, fix) => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure({ items: [], cause }) });
    const { container } = await mountSettings("Codex");
    expect(paragraphs(openMark(container)).slice(0, 3)).toEqual(["Could not finish turning sharing on.", line, fix]);
  });

  it("describes the switch by the title alone, a copy that is there while the popover is closed", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure({ items: [], cause: "no_space" }) });
    const { container } = await mountSettings("Codex");
    const sharing = sharingSwitches(container)[0]!;
    expect(captionOf(container, sharing)).toBe("Could not finish turning sharing on.");
    expect(document.querySelector("[data-status-mark-popover]")).toBeNull();
    for (const id of sharing.getAttribute("aria-describedby")!.split(" ")) expect(container.querySelector(`[id="${id}"]`)).not.toBeNull();
  });

  it("has only the raw reason after the sentences for a whole failure whose cause has no line of its own", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure({ items: [], cause: "other" }) });
    const { container } = await mountSettings("Codex");
    const popover = openMark(container);
    expect(paragraphs(popover)).toEqual(["Could not finish turning sharing on.", "Some files or folders could not be changed.", "Turn the switch on again to try once more."]);
    expect(popover.querySelector("span.block")).toBeNull();
    expect(popover.textContent).toContain("could not link skills");
  });

  it("has only its sentences and the guidance for a whole failure with no cause line and no raw reason", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure({ items: [], cause: "other", detail: undefined }) });
    const { container } = await mountSettings("Codex");
    expect(openMark(container).textContent).toBe("Could not finish turning sharing on.Some files or folders could not be changed.Turn the switch on again to try once more.");
  });

  it("gives the cause, then the raw reason, when the change could not start", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure({ items: [], cause: "not_found", detail: "the directory is not readable" }) });
    const { container } = await mountSettings("Codex");
    const popover = openMark(container);
    expect(paragraphs(popover)).toEqual(["Could not finish turning sharing on.", "A directory needed for sharing could not be found.", "Turn the switch on again to try once more."]);
    expect(popover.querySelector("span.block")).toBeNull();
    expect(popover.querySelector("code")?.textContent).toBe("the directory is not readable");
  });

  it("shows the same warning without a reason after a restart, with the guidance under the title", async () => {
    unfinished();
    const { container } = await mountSettings("Codex");
    expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
    expect(paragraphs(openMark(container))).toEqual(["The last attempt to turn sharing on did not finish.", "Turn the switch on again to try once more."]);
    expect(buttonNamed(container, "Try again")).toBeUndefined();
    const [sharing] = sharingSwitches(container);
    expect(sharing!.checked).toBe(false);
    expect(captionOf(container, sharing!)).toBe("The last attempt to turn sharing on did not finish.");
  });

  it("does not take a failure for an unfinished change when the daemon does not say it is incomplete", async () => {
    accountSharing.set("codex-work", { account: "codex-work", failure: failure() });
    const { container } = await mountSettings("Codex");
    expect(mark(container)).toBeNull();
    expect(captionOf(container, sharingSwitches(container)[0]!)).toBe("");
  });

  it("shows the recorded position and asks to turn sharing off and on when the default directory has moved", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure({ cause: "default_moved", items: [] }) });
    const { container } = await mountSettings("Codex");
    const popover = openMark(container);
    expect(paragraphs(popover)).toEqual(["Could not finish turning sharing on.", "The default account’s directory has moved since sharing was turned on.", "Turn the switch off, then on again."]);
    expect(popover.textContent).toContain("could not link skills");
    expect(sharingSwitches(container)[0]!.checked).toBe(true);
    await act(async () => sharingSwitches(container)[0]!.click());
    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_account_sharing", account: "codex-work", share: false });
  });

  it("speaks a failure that follows a change it saw run, and not one restated by an unrelated update", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure() });
    const { container, render } = await mountSettings("Codex");
    expect(await spoken(container)).toBe("");
    accountSharing.set("codex-work", { account: "codex-work", failure: failure(), in_use: { sessions: 1, signing_in: false } });
    render();
    expect(await spoken(container)).toBe("");
    accountSharing.set("codex-work", { account: "codex-work", progress: { share: true, phase: "changing" as const, done: 1, total: 3, item: "skills" } });
    render();
    expect(mark(container)).toBeNull();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure({ detail: "again" }) });
    render();
    expect(mark(container)).not.toBeNull();
    expect(await spoken(container)).toBe("Could not finish turning sharing on.");
  });

  it("keeps the mark as lasting state for a change asked for here, and leaves the announcing to the error alert dialog", async () => {
    const { container, render } = await mountSettings("Codex");
    let fail!: (error: Error) => void;
    request.mockReturnValueOnce(new Promise((_, reject) => void (fail = reject)));
    act(() => sharingSwitches(container)[0]!.click());
    // The daemon records the failure before it answers the request.
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure() });
    render();
    expect(mark(container)).not.toBeNull();
    await act(async () => fail(new DaemonRequestError("failed", "account_sharing_failed", { account: "codex-work", name: "codex-work", items: "[]", detail: "x" })));
    expect(reported()).toEqual([["Could not change sharing", "account_sharing_failed"]]);
    expect(mark(container)).not.toBeNull();
    expect(await spoken(container)).toBe("");
  });

  it("speaks a change another window starts and ends unfinished, though an earlier change was asked for here", async () => {
    const { container, render } = await mountSettings("Codex");
    let fail!: (error: Error) => void;
    request.mockReturnValueOnce(new Promise((_, reject) => void (fail = reject)));
    act(() => sharingSwitches(container)[0]!.click());
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure() });
    render();
    await act(async () => fail(new DaemonRequestError("failed", "account_sharing_failed", { account: "codex-work", name: "codex-work", items: "[]", detail: "x" })));
    expect(await spoken(container)).toBe("");
    // The next change is started elsewhere: progress arrives with no request of this window's.
    accountSharing.set("codex-work", { account: "codex-work", progress: { share: true, phase: "changing" as const, done: 1, total: 3, item: "skills" } });
    render();
    expect(mark(container)).toBeNull();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure({ detail: "again" }) });
    render();
    expect(mark(container)).not.toBeNull();
    expect(await spoken(container)).toBe("Could not finish turning sharing on.");
  });

  it("shows the recorded position while the change runs again, and no warning", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", progress: { share: true, phase: "changing" as const, done: 0, total: 3 } });
    const { container } = await mountSettings("Codex");
    expect(mark(container)).toBeNull();
    expect(sharingSwitches(container)[0]!.checked).toBe(true);
  });

  it("moves focus from the mark to the switch when the warning goes, and leaves other focus alone", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure() });
    const { container, render } = await mountSettings("Codex");
    act(() => mark(container)!.focus());
    accountSharing.delete("codex-work");
    settings.accounts[1] = { ...settings.accounts[1]!, share_default_config_incomplete: false };
    render();
    expect(mark(container)).toBeNull();
    expect(document.activeElement).toBe(sharingSwitches(container)[0]);
    // Focus that was elsewhere stays there.
    settings.accounts[1] = { ...settings.accounts[1]!, share_default_config_incomplete: true };
    render();
    const outside = container.querySelector<HTMLElement>('button[aria-label="About sharing the default configuration"]')!;
    act(() => outside.focus());
    settings.accounts[1] = { ...settings.accounts[1]!, share_default_config_incomplete: false };
    render();
    expect(mark(container)).toBeNull();
    expect(document.activeElement).toBe(outside);
  });

  it("reports a retry the daemon refuses", async () => {
    unfinished(false);
    const { container } = await mountSettings("Codex");
    request.mockRejectedValueOnce(new DaemonRequestError("busy", "account_in_use", { account: "codex-work", name: "codex-work", sessions: "1", signing_in: "false" }));
    await act(async () => sharingSwitches(container)[0]!.click());
    expect(reported()).toEqual([["Could not change sharing", "account_in_use"]]);
  });

  it("holds the switch back while sessions of the account run, and says why", async () => {
    unfinished();
    accountSharing.set("codex-work", { account: "codex-work", failure: failure(), in_use: { sessions: 1, signing_in: false } });
    const { container } = await mountSettings("Codex");
    const [sharing] = sharingSwitches(container);
    expect(sharing!.getAttribute("aria-disabled")).toBe("true");
    act(() => sharing!.click());
    expect(request).not.toHaveBeenCalled();
    expect(captionOf(container, sharing!)).toContain("1 of this account’s sessions is working");
  });
});

describe("an unavailable control keeps keyboard focus", () => {
  it("leaves the switch focusable and focused when it is locked, and a press or Space changes nothing", async () => {
    accountSharing.set("codex-work", { account: "codex-work", in_use: { sessions: 1, signing_in: false } });
    const { container } = await mountSettings("Codex");
    const [sharing] = sharingSwitches(container);
    expect(sharing!.disabled).toBe(false);
    expect(sharing!.tabIndex).toBe(0);
    sharing!.focus();
    act(() => sharing!.click());
    expect(request).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(sharing);
  });

  it("keeps the switch focused while the request is under way and while a change runs", async () => {
    let finish!: (value: object) => void;
    const { container, render } = await mountSettings("Codex");
    request.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const [sharing] = sharingSwitches(container);
    sharing!.focus();
    act(() => sharing!.click());
    expect(document.activeElement).toBe(sharing);
    expect(sharing!.getAttribute("aria-disabled")).toBe("true");
    accountSharing.set("codex-work", { account: "codex-work", progress: { share: true, phase: "changing" as const, done: 0, total: 2 } });
    render();
    expect(document.activeElement).toBe(sharingSwitches(container)[0]);
    await act(async () => finish({}));
    expect(document.activeElement).toBe(sharingSwitches(container)[0]);
  });
});

it("shows the switch off again for an account that was repointed and no longer shares", async () => {
  settings.accounts[1]!.share_default_config = true;
  const { container, render } = await mountSettings("Codex");
  expect(sharingSwitches(container)[0]!.checked).toBe(true);
  settings.accounts[1] = { ...settings.accounts[1]!, config_dir: "/home/dev/.elsewhere", share_default_config: false, share_default_config_incomplete: false };
  render();
  expect(sharingSwitches(container)[0]!.checked).toBe(false);
});

it("offers no sharing switch in the settings of the default account", async () => {
  const { container } = mount("Codex");
  const dialog = await openSettingsOf(container, "default:codex");
  expect(dialog.querySelector("[data-sharing-account]")).toBeNull();
  expect(sharingSwitches(dialog)).toHaveLength(0);
});
