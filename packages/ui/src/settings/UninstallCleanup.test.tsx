// @vitest-environment jsdom
import "../input/focusGuard";
import { act } from "react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import type { HostFacts, PlatformAdapter, ShellSystemChange, ShellSystemChangeResult, ShellSystemChangesCleared, ShellSystemChangesFailure, ShellSystemChangesListed, SystemChangesCapability } from "../platform";
import type { ClientRequestBody, HostInfo } from "../protocol";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { currentLanguage, setLanguage } from "../i18n/language";
import type { Language } from "../i18n/languages";
import { windowAttention } from "../components/announce/windowTestSupport";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { fakeTimers, flushPromises } from "../test/clock";
import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import * as dom from "../test/dom";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { UninstallCleanup } from "./UninstallCleanup";
import { SettingsFocusContext } from "./useSectionRefocus";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => acknowledgeAllAlerts());

const KEY: ShellSystemChange = { kind: "device_key", id: "e1", name: "Build server", host: "build.example", user: "dev", onServer: true };
const OTHER_KEY: ShellSystemChange = { kind: "device_key", id: "e2", name: "Staging", host: "staging.example", user: "deploy", onServer: true };
const PRIVATE_KEY: ShellSystemChange = { kind: "device_key", id: "e3", name: "Old server", host: "old.example", user: "dev", onServer: false };
const LOGIN: ShellSystemChange = { kind: "launch_at_login", turnedOff: false };
const LOGIN_TURNED_OFF: ShellSystemChange = { kind: "launch_at_login", turnedOff: true };
const LINK: ShellSystemChange = { kind: "command_line", path: "/usr/local/bin/octoboardd" };
const SHARING: ShellSystemChange = { kind: "account_sharing", account: "acct-1", name: "Work", agent: "claude" };
const OTHER_SHARING: ShellSystemChange = { kind: "account_sharing", account: "acct-2", name: "Personal", agent: "claude" };
/** This computer's daemon not answering, as the shell reports it. */
const DAEMON_DOWN: ShellSystemChangesFailure = { kind: "daemon", detail: "The connection to the daemon was closed." };
/** The daemon's host as its snapshot says it, but for whether the daemon is a desktop application's own. */
const DAEMON_HOST = { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false } as const;

/** The shell's capability, listing `changes` (and `listFailed` as the parts that cannot tell) until cleared and then
 * what `results` left; `failed` are the parts it answers as failed whole. `add` lists one more, and `set` replaces
 * what the next list answers. */
function shellWith(
  changes: ShellSystemChange[],
  { results = changes.map((change) => ({ change, outcome: { result: "cleared" } as const })), failed = [], listFailed = [] }: { results?: ShellSystemChangeResult[]; failed?: ShellSystemChangesFailure[]; listFailed?: ShellSystemChangesFailure[] } = {},
) {
  let listed = changes;
  let unread = listFailed;
  return {
    list: vi.fn(async (): Promise<ShellSystemChangesListed> => ({ changes: listed, failed: unread })),
    clearAll: vi.fn(async (): Promise<ShellSystemChangesCleared> => {
      listed = results.filter((result) => result.outcome.result !== "cleared").map((result) => result.change);
      return { results, failed };
    }),
    add: (change: ShellSystemChange) => {
      listed = [...listed, change];
    },
    set: (next: ShellSystemChange[], nextFailed: ShellSystemChangesFailure[] = []) => {
      listed = next;
      unread = nextFailed;
    },
  };
}

/** Mounts the row: in the desktop application's own window with `shell`, in a remote daemon's window with `kind`
 * "tauri" and none, and otherwise in a browser, on a desktop application's own daemon with `applicationDaemon`, as
 * the daemon says of itself in its snapshot. The row sits in a Settings section whose entry is `entry`, where focus
 * goes once the row's own button cannot take it.
 * `sent` holds every request the daemon was sent, which is none. */
function mount({ shell, kind = shell ? "tauri" : "browser", host = MAC_HOST, applicationDaemon = false, connected = true }: { shell?: SystemChangesCapability; kind?: PlatformAdapter["kind"]; host?: HostFacts; applicationDaemon?: boolean; connected?: boolean } = {}) {
  const sent: ClientRequestBody[] = [];
  const daemonHost: HostInfo = { ...DAEMON_HOST, application: applicationDaemon };
  const daemon = testDaemon({
    state: { homeDir: "/home/dev", ...(connected && { host: daemonHost }) },
    request: async (body: ClientRequestBody) => {
      sent.push(body);
      throw new Error(`unexpected request ${body.type}`);
    },
  });
  const platform: PlatformAdapter = { kind, host, systemChanges: shell };
  const entry = document.createElement("button");
  entry.textContent = "Cleanup";
  document.body.append(entry);
  onTestFinished(() => entry.remove());
  const container = mountHost(<SettingsFocusContext.Provider value={() => entry}><UninstallCleanup /></SettingsFocusContext.Provider>, { daemon, platform });
  return { container, sent, daemon, entry };
}

/** Lets the read and the clearing, each a chain of promises, run to their end. */
const settle = flushPromises;
const click = (element: Element | null | undefined) => act(async () => void (element as HTMLElement | undefined)?.click());
const button = (root: ParentNode, text: string) => [...root.querySelectorAll("button")].find((candidate) => candidate.textContent === text);
const dialog = () => document.querySelector<HTMLElement>('[role="alertdialog"], [role="dialog"]');
/** The trigger of a text disclosure in `root`, by the label it reads: the button that shows and hides what it folds away. */
const trigger = (root: ParentNode, text: string) => [...root.querySelectorAll<HTMLButtonElement>("button[aria-expanded]")].find((candidate) => candidate.textContent === text);
/** The disclosure that folds a list away, by the count its trigger reads: its trigger and the content under it, which
 * stays in the document (hidden) while it is collapsed. */
const fold = (text: string) => trigger(dialog()!, text)?.closest<HTMLElement>(".disclosure") ?? undefined;
/** A text disclosure in `root`, if there is one. */
const disclosureIn = (root: ParentNode) => root.querySelector<HTMLElement>(".disclosure");
/** Whether a disclosure is expanded, as its trigger says. */
const isOpen = (disclosure: Element) => disclosure.querySelector("button[aria-expanded]")!.getAttribute("aria-expanded") === "true";
/** The lines of the list a fold holds. */
const folded = (text: string) => [...fold(text)!.querySelectorAll("ul > li")];
const foldedTexts = (text: string) => folded(text).map((line) => line.textContent);

/** Types the word the confirmation asks for into its field. */
const typeWord = () => dom.type(dialog()!.querySelector("input")!, "REMOVE");

/** Presses the row's button and waits for the confirmation's own read. */
async function open(container: HTMLElement): Promise<void> {
  await click(button(container, "Remove all"));
  await settle();
}

/** Opens the confirmation, types its word and confirms, which leaves the result on screen. */
async function removeAll(container: HTMLElement): Promise<void> {
  await open(container);
  typeWord();
  await click(button(dialog()!, "Remove all"));
  await settle();
}

it("is one row that says what it is for and how many items are in effect, without listing them", async () => {
  const { container, sent } = mount({ shell: shellWith([KEY, LOGIN, LINK, SHARING]) });
  await settle();
  const said = container.textContent ?? "";
  expect(said).toContain("Clean up before uninstalling");
  expect(said).toContain("Removes what the app’s features added outside its own folders, for when you are uninstalling");
  // Said once: one item is undone at its own switch.
  expect(said).toContain("To undo just one, turn off its own setting.");
  expect(said).toContain("4 items in effect");
  // Nothing of the items themselves is on the page.
  expect(container.querySelectorAll("ul, li")).toHaveLength(0);
  expect(said).not.toContain("build.example");
  expect(said).not.toContain("/usr/local/bin/octoboardd");
  expect(said).not.toContain("Work");
  // The one action is the only control.
  expect([...container.querySelectorAll("button")].map((candidate) => candidate.textContent)).toEqual(["Remove all"]);
  expect(container.querySelectorAll('a, input, [role="switch"], [role="checkbox"], [role="menuitem"]')).toHaveLength(0);
  // The shell is the one end that is asked.
  expect(sent).toHaveLength(0);
});

it("counts one item in the singular", async () => {
  const { container } = mount({ shell: shellWith([LOGIN]) });
  await settle();
  expect(container.textContent).toContain("1 item in effect");
});

it("draws the row's action as a soft danger button, the confirmation in the dialog being the strong one", async () => {
  const { container } = mount({ shell: shellWith([LOGIN]) });
  await settle();
  const action = button(container, "Remove all")!;
  expect(action.classList.contains("button--danger-soft")).toBe(true);
  expect(action.classList.contains("button--danger")).toBe(false);
});

it("says there is nothing to remove, and holds the action disabled without a reason of its own", async () => {
  const shell = shellWith([]);
  const { container } = mount({ shell });
  await settle();
  expect(container.textContent).toContain("Nothing to remove.");
  const action = button(container, "Remove all")!;
  expect(action.disabled).toBe(true);
  await click(action);
  expect(dialog()).toBeNull();
  expect(shell.clearAll).not.toHaveBeenCalled();
});

it("counts in the dialog what will be removed and keeps the list folded away until its count is pressed", async () => {
  const { container } = mount({ shell: shellWith([KEY, OTHER_KEY, LOGIN, LINK, SHARING]) });
  await settle();
  await open(container);
  const count = fold("5 items will be removed")!;
  expect(isOpen(count)).toBe(false);
  // The count after its triangle is what is pressed: the text disclosure for content shown for reading, not a form's
  // quiet button.
  const countTrigger = trigger(dialog()!, "5 items will be removed")!;
  expect(countTrigger.querySelector("svg[aria-hidden=true]")).not.toBeNull();
  await click(countTrigger);
  expect(isOpen(count)).toBe(true);
  const listed = foldedTexts("5 items will be removed");
  expect(listed).toHaveLength(5);
  expect(listed[0]).toContain("This computer’s key");
  expect(listed[0]).toContain("dev@build.example");
  // Two keys are told apart by whom they sign in as.
  expect(listed[1]).toContain("deploy@staging.example");
  expect(listed[2]).toBe("Launch at login");
  expect(listed[3]).toContain("Command-line tool");
  expect(listed[3]).toContain("/usr/local/bin/octoboardd");
  expect(listed[4]).toContain("Shared default configuration");
  expect(listed[4]).toContain("Work");
  // No line holds a control that acts on its item: the one button is the sharing line's note, which only explains.
  expect(dialog()!.querySelectorAll("ul a, ul input, ul [data-inline-path]")).toHaveLength(0);
  expect([...dialog()!.querySelectorAll("ul button")].map((mark) => mark.closest("li"))).toEqual([folded("5 items will be removed")[4]]);
  await click(countTrigger);
  expect(isOpen(count)).toBe(false);
});

it("lists every item as one line of one shape that names its kind, with no grouping and nothing that wraps", async () => {
  const { container } = mount({ shell: shellWith([KEY, OTHER_KEY, PRIVATE_KEY, LINK]) });
  await settle();
  await open(container);
  const listed = folded("4 items will be removed");
  // Each line names its kind itself: three keys are three lines that begin alike.
  expect(listed.map((line) => line.querySelector("span")!.textContent)).toEqual(["This computer’s key", "This computer’s key", "This computer’s key", "Command-line tool"]);
  // One row per line: the kind, the fact and the qualifier side by side, none of them allowed onto a second line.
  const rowOf = (line: Element) => line.querySelector(":scope > div > div")!;
  for (const line of listed) {
    const row = rowOf(line);
    expect(line.children).toHaveLength(1);
    expect(row.className).toContain("flex");
    expect(row.className).not.toContain("flex-wrap");
    expect(row.className).not.toContain("grid");
  }
  expect(rowOf(listed[2]).children).toHaveLength(3);
  expect(listed[2].textContent).toBe("This computer’s keydev@old.example, Private key only");
  // Nothing has happened yet: no line carries the glyph of an outcome.
  expect(listed.flatMap((line) => [...line.querySelectorAll("svg")])).toHaveLength(0);
});

it("counts one item to remove in the singular", async () => {
  const { container } = mount({ shell: shellWith([LOGIN]) });
  await settle();
  await open(container);
  expect(fold("1 item will be removed")).toBeDefined();
});

it("marks a private key left behind and a login entry turned off in the system's list with a qualifier on their line", async () => {
  const { container } = mount({ shell: shellWith([PRIVATE_KEY, LOGIN_TURNED_OFF]) });
  await settle();
  // On the page they are only counted.
  expect(container.textContent).toContain("2 items in effect");
  expect(container.textContent).not.toContain("Private key only");
  await open(container);
  const listed = foldedTexts("2 items will be removed");
  expect(listed[0]).toContain("dev@old.example");
  expect(listed[0]).toContain("Private key only");
  expect(listed[1]).toContain("Launch at login");
  expect(listed[1]).toContain("Turned off, still there");
});

it("asks first, in one sentence saying what the cleanup does and one line saying what is kept", async () => {
  const shell = shellWith([KEY, LOGIN, SHARING]);
  const { container } = mount({ shell });
  await settle();
  await open(container);
  expect(shell.clearAll).not.toHaveBeenCalled();
  const asked = dialog()!.textContent ?? "";
  expect(asked).toContain("Remove all before uninstalling");
  expect(asked).toContain("this removes what the app’s features added outside its own folders and turns those features off.");
  expect(asked).toContain("3 items will be removed");
  expect(asked).toContain("The app’s own data and the conversation history that sharing moved are kept.");
  // What the row already says is not said again.
  expect(asked).not.toContain("A single one is undone");
  await click(button(dialog()!, "Cancel"));
  expect(dialog()).toBeNull();
  expect(shell.clearAll).not.toHaveBeenCalled();
});

it("holds Confirm until the word it asks for is typed, and clears nothing before", async () => {
  const shell = shellWith([LOGIN, SHARING]);
  const { container, sent } = mount({ shell });
  await settle();
  await open(container);
  expect(dialog()!.textContent).toContain("Type REMOVE to confirm");
  const confirm = () => button(dialog()!, "Remove all")!;
  expect(confirm().disabled).toBe(true);
  await click(confirm());
  await settle();
  expect(shell.clearAll).not.toHaveBeenCalled();
  // Another word does not do.
  dom.type(dialog()!.querySelector("input")!, "DELETE");
  expect(confirm().disabled).toBe(true);
  // Compared ignoring case.
  dom.type(dialog()!.querySelector("input")!, "remove");
  expect(confirm().disabled).toBe(false);
  await click(confirm());
  await settle();
  expect(shell.clearAll).toHaveBeenCalledTimes(1);
  expect(sent).toHaveLength(0);
});

it("speaks of sharing's history only when an account shares, and says what happens to its sessions behind a mark at the end of its line", async () => {
  const shell = shellWith([LOGIN]);
  const { container } = mount({ shell });
  await settle();
  await open(container);
  expect(dialog()!.textContent).toContain("The app’s own data is kept.");
  expect(dialog()!.textContent).not.toContain("conversation history");
  expect(dialog()!.textContent).not.toContain("idle sessions");
  await click(button(dialog()!, "Cancel"));
  shell.add(SHARING);
  await open(container);
  const hint = "This account’s idle sessions are restarted when it stops sharing; their conversations continue.";
  const [login, sharing] = folded("2 items will be removed");
  // No sentence of its own under the list: the hint is a mark, a button named by what it says, on the sharing line
  // alone and last on it.
  expect(fold("2 items will be removed")!.querySelectorAll("p")).toHaveLength(0);
  const mark = sharing.querySelector<HTMLElement>("button")!;
  expect(mark.getAttribute("aria-label")).toBe(hint);
  expect(mark.closest("div")!.lastElementChild!.contains(mark)).toBe(true);
  expect(login.querySelector("button")).toBeNull();
  // Opened, it says the sentence.
  await click(mark);
  expect(document.body.textContent).toContain(hint);
});

it("shows no such mark on a line of the result", async () => {
  const inUse: ShellSystemChangeResult = { change: OTHER_SHARING, outcome: { result: "account_sharing_left", failure: { code: "account_in_use", params: { name: "Personal", sessions: "1" }, message: "in use" } } };
  const { container } = mount({ shell: shellWith([SHARING, OTHER_SHARING], { results: [{ change: SHARING, outcome: { result: "cleared" } }, inUse] }) });
  await settle();
  await removeAll(container);
  expect(dialog()!.textContent).toContain("1 removed, 1 could not be removed.");
  expect(dialog()!.querySelectorAll("li button")).toHaveLength(0);
});

it("asks the shell again as the dialog opens, so that it counts what appeared since the row was read", async () => {
  const shell = shellWith([LOGIN]);
  const { container } = mount({ shell });
  await settle();
  shell.add(LINK);
  shell.add(SHARING);
  // The shell's answer to the read the dialog opens with is held back, to see Confirm held meanwhile.
  let answer!: (listed: ShellSystemChangesListed) => void;
  shell.list.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
  await click(button(container, "Remove all"));
  // Pending, as the button of a running action is: it keeps its name, and is not a submit button meanwhile.
  expect(button(dialog()!, "Remove all")!.getAttribute("aria-busy")).toBe("true");
  await act(async () => answer({ changes: [LOGIN, LINK, SHARING], failed: [] }));
  await settle();
  expect(button(dialog()!, "Remove all")!.hasAttribute("aria-busy")).toBe(false);
  expect(foldedTexts("3 items will be removed")).toHaveLength(3);
  expect(shell.list).toHaveBeenCalledTimes(2);
  // The row follows the same read.
  expect(container.textContent).toContain("3 items in effect");
});

it("closes the dialog when the read it opens with finds nothing left to remove", async () => {
  const shell = shellWith([LOGIN]);
  const { container } = mount({ shell });
  await settle();
  // Turned off at its own switch meanwhile.
  shell.set([]);
  await open(container);
  expect(dialog()).toBeNull();
  expect(shell.clearAll).not.toHaveBeenCalled();
  expect(container.textContent).toContain("Nothing to remove.");
});

it("clears everything on confirming, says so, and folds the removed items away under their count", async () => {
  const shell = shellWith([KEY, LOGIN, LINK, SHARING]);
  const { container } = mount({ shell });
  await settle();
  await removeAll(container);
  expect(shell.clearAll).toHaveBeenCalledTimes(1);
  const said = dialog()!.textContent ?? "";
  expect(said).toContain("Result of the cleanup");
  expect(said).toContain("Everything was removed.");
  expect(said).not.toContain("could not be removed");
  expect(said).not.toContain("Not removed");
  const count = fold("4 removed")!;
  expect(isOpen(count)).toBe(false);
  const listed = foldedTexts("4 removed");
  expect(listed).toHaveLength(4);
  // Every line is led by the success glyph, with its word for a screen reader.
  for (const line of folded("4 removed")) {
    expect(line.firstElementChild!.querySelector("svg")).not.toBeNull();
    expect(line.firstElementChild!.querySelector(".sr-only")!.textContent).toBe("Removed");
  }
  expect(listed[0]).toContain("dev@build.example");
  expect(listed[2]).toContain("/usr/local/bin/octoboardd");
  expect(listed[3]).toContain("Work");
  // Every list in the result is the folded one: nothing was left.
  expect(dialog()!.querySelectorAll("ul")).toHaveLength(1);
  // One button, which closes; the row has been read again.
  expect(button(dialog()!, "Remove all")).toBeUndefined();
  await click(button(dialog()!, "Close"));
  expect(dialog()).toBeNull();
  expect(container.textContent).toContain("Nothing to remove.");
  expect(button(container, "Remove all")!.disabled).toBe(true);
});

it("shows the items that were not removed open, each with what stays, and the removed ones folded away", async () => {
  const keyLeft: ShellSystemChangeResult = {
    change: KEY,
    outcome: {
      result: "device_key_left",
      removal: {
        line: { result: "left_on_host", mark: "Octoboard on Desk", file: "~/.ssh/authorized_keys", host: "build.example", user: "dev", detail: "connection refused" },
        privateKeyLeft: { service: "dev.octoboard.app", account: "e1", host: "build.example", user: "dev", detail: "the keychain is locked" },
      },
    },
  };
  const linkLeft: ShellSystemChangeResult = { change: LINK, outcome: { result: "command_line_left", error: { code: "refused", path: "/usr/local/bin/octoboardd", detail: "User canceled" } } };
  const loginLeft: ShellSystemChangeResult = { change: LOGIN, outcome: { result: "launch_at_login_left", detail: "Operation not permitted" } };
  const inUse: ShellSystemChangeResult = { change: SHARING, outcome: { result: "account_sharing_left", failure: { code: "account_in_use", params: { name: "Work", sessions: "1" }, message: "the account is in use" } } };
  const cleared: ShellSystemChangeResult[] = [OTHER_KEY, OTHER_SHARING].map((change) => ({ change, outcome: { result: "cleared" } }));
  const shell = shellWith([KEY, OTHER_KEY, LOGIN, LINK, SHARING, OTHER_SHARING], { results: [keyLeft, cleared[0], loginLeft, linkLeft, inUse, cleared[1]] });
  const { container } = mount({ shell });
  await settle();
  await removeAll(container);
  const said = dialog()!.textContent ?? "";
  // One line says how it went, under the warning glyph.
  expect(said).toContain("2 removed, 4 could not be removed.");
  expect(dialog()!.querySelector("p svg")).not.toBeNull();
  // The removed ones are folded away, and are not among the open lines.
  const count = fold("2 removed")!;
  expect(isOpen(count)).toBe(false);
  const panel = count;
  expect(foldedTexts("2 removed").join(" ")).toContain("deploy@staging.example");
  const open = [...dialog()!.querySelectorAll("ul > li")].filter((line) => !panel.contains(line));
  expect(open).toHaveLength(4);
  expect(open[0].textContent).toContain("dev@build.example");
  // Each entry: its line, the reason and the remedy written under it, and the rest folded under Details, which is the
  // text disclosure and not an alert's Details popover.
  const shown = (entry: Element) => [...entry.children].filter((part) => disclosureIn(part) === null).map((part) => part.textContent);
  // Each is a block of its own: the failure glyph with its word for a screen reader, the item's line as the title,
  // and the reason and remedy under it.
  expect(shown(open[0])).toEqual(["Not removed", "This computer’s keydev@build.example", "Could not be removed from the server; the line may still be there. This computer’s credential store did not remove the private key."]);
  // Where the user removes the login entry the system kept, System Settings' Login Items as a path, is in the block's
  // sentence, not behind Details; so is why the link stayed.
  expect(shown(open[1])).toHaveLength(3);
  expect(shown(open[1])[2]).toContain("so it is still in that list. Remove it in");
  expect(shown(open[1])[2]).toContain("Login Items");
  expect(shown(open[2])).toEqual(["Not removed", "Command-line tool/usr/local/bin/octoboardd", "The link could not be removed.", "The system did not allow the change to /usr/local/bin/octoboardd, so it was left as it was."]);
  expect(shown(open[3])[2]).toContain("cannot be changed while 1 of its sessions is working");
  for (const entry of open) {
    expect(entry.className).toContain("panel-edge");
    expect(entry.firstElementChild!.querySelector("svg")).not.toBeNull();
    expect(entry.children[1].querySelector("span")!.className).toContain("font-medium");
  }
  for (const entry of open.slice(0, 3)) {
    const details = disclosureIn(entry)!;
    expect(isOpen(details)).toBe(false);
    expect(trigger(details, "Details")).toBeDefined();
  }
  expect(disclosureIn(open[3])).toBeNull();
  // Every Details is the trigger of an entry's disclosure; none is an alert's Details popover.
  const detailsButtons = [...dialog()!.querySelectorAll("button")].filter((candidate) => candidate.textContent === "Details");
  expect(detailsButtons).toEqual(open.slice(0, 3).map((entry) => disclosureIn(entry)!.querySelector("button[aria-expanded]")));
  // Both parts of the key: its line on the server, by its mark and its file, and its private key.
  expect(open[0].textContent).toContain("the line marked Octoboard on Desk may still be in ~/.ssh/authorized_keys for dev there");
  expect(open[0].textContent).toContain("did not remove the private key kept for dev on build.example");
  expect(said).not.toContain("The private key was removed from this computer.");
  // Details keep only the rest: the system's own words.
  expect(disclosureIn(open[1])!.textContent).not.toContain("Remove it in");
  expect(disclosureIn(open[2])!.textContent).not.toContain("did not allow the change");
  expect(disclosureIn(open[2])!.textContent).toContain("User canceled");
  // The daemon's refusal, in the application's words.
  expect(open[3].textContent).toContain("Work");
  expect(open[3].textContent).not.toBe("Shared default configurationClaude CodeWork");
  // The system's own words are under Details, with the explanation; the daemon's raw text is nowhere.
  expect(disclosureIn(open[0])!.textContent).toContain("connection refused");
  expect(shown(open[1]).join(" ")).not.toContain("Operation not permitted");
  expect(disclosureIn(open[1])!.textContent).toContain("Operation not permitted");
  expect(said).not.toContain("the account is in use");
  // What was left is still in effect, so the row counts it and the action stays available.
  await click(button(dialog()!, "Close"));
  expect(container.textContent).toContain("4 items in effect");
  expect(button(container, "Remove all")!.disabled).toBe(false);
});

it("counts one item that could not be removed in the singular, with nothing folded when none was removed", async () => {
  const loginLeft: ShellSystemChangeResult = { change: LOGIN, outcome: { result: "launch_at_login_left", detail: "access denied" } };
  const { container } = mount({ shell: shellWith([LOGIN], { results: [loginLeft] }), host: WINDOWS_HOST });
  await settle();
  await removeAll(container);
  const said = dialog()!.textContent ?? "";
  expect(said).toContain("1 item could not be removed.");
  expect(said).not.toContain("0 removed");
  // The only disclosure is the entry's Details: no removed list where nothing was removed.
  expect([...dialog()!.querySelectorAll("button[aria-expanded]")].map((candidate) => candidate.textContent)).toEqual(["Details"]);
  // A Windows user is sent to Task Manager's Startup apps for a login entry the system kept, as one path, in the
  // block's sentence and not behind Details.
  expect(said).toContain("Remove it in Task Manager > Startup apps.");
  const path = dialog()!.querySelector("[data-inline-path]")!;
  expect(path.textContent).toBe("Task Manager > Startup apps");
  expect(disclosureIn(dialog()!.querySelector("ul > li")!)!.contains(path)).toBe(false);
});

it("writes the remedy of an account that still shares under its reason, and keeps Details for the rest", async () => {
  const unreachable: ShellSystemChangeResult = { change: SHARING, outcome: { result: "account_sharing_left", failure: { code: "config_dir_unreachable", params: { agent: "claude", path: "/home/dev/.claude-work" }, message: "not a directory" } } };
  const { container } = mount({ shell: shellWith([SHARING], { results: [unreachable] }) });
  await settle();
  await removeAll(container);
  const entry = dialog()!.querySelector("ul > li")!;
  const shown = [...entry.children].filter((part) => disclosureIn(part) === null).map((part) => part.textContent);
  expect(shown[2]).toContain("is not a directory the daemon can reach");
  expect(shown[3]).toBe("Recreate it, or point the account at a different directory.");
  expect(disclosureIn(entry)?.textContent ?? "").not.toContain("Recreate it");
});

it("says the private key went where only the key's line stays on the server, and nothing of the line where it was never there", async () => {
  const lineLeft: ShellSystemChangeResult = {
    change: KEY,
    outcome: { result: "device_key_left", removal: { line: { result: "left_on_host", mark: "Octoboard on Desk", file: null, host: "build.example", user: "dev", detail: "unreachable" }, privateKeyLeft: null } },
  };
  const privateLeft: ShellSystemChangeResult = {
    change: PRIVATE_KEY,
    outcome: { result: "device_key_left", removal: { line: { result: "not_installed" }, privateKeyLeft: { service: "dev.octoboard.app", account: "e3", host: "old.example", user: "dev", detail: "locked" } } },
  };
  const { container } = mount({ shell: shellWith([KEY, PRIVATE_KEY], { results: [lineLeft, privateLeft] }) });
  await settle();
  await removeAll(container);
  const said = dialog()!.textContent ?? "";
  expect(said).toContain("may still be in the authorized keys of dev there");
  expect(said).toContain("The private key was removed from this computer.");
  expect(said).toContain("did not remove the private key kept for dev on old.example");
  // The qualifier stays on the line of the private key left behind.
  expect(said).toContain("Private key only");
  // No line was installed for the private key left behind, so none is said to have been removed.
  expect(said).not.toContain("The line was removed from the server.");
});

it("shows a key whose removal left the server's file incomplete as not removed, naming the copy the server kept", async () => {
  const incomplete: ShellSystemChangeResult = {
    change: KEY,
    outcome: {
      result: "device_key_left",
      removal: { line: { result: "file_incomplete", host: "build.example", user: "dev", copy: "~/.ssh/authorized_keys.octoboard", detail: "No space left on device" }, privateKeyLeft: null },
    },
  };
  const { container } = mount({ shell: shellWith([KEY], { results: [incomplete] }) });
  await settle();
  await removeAll(container);
  const said = dialog()!.textContent ?? "";
  expect(said).toContain("1 item could not be removed.");
  expect(said).not.toContain("0 removed");
  expect(said).toContain("The server’s authorized keys may be incomplete.");
  expect(said).toContain("~/.ssh/authorized_keys.octoboard");
  expect(said).not.toContain("The line was removed from the server.");
});

it("says the application failed when its clearing does not answer, without claiming anything was removed", async () => {
  const shell: SystemChangesCapability = { list: async () => ({ changes: [LOGIN, SHARING], failed: [] }), clearAll: () => Promise.reject(new Error("the shell did not answer")) };
  const { container } = mount({ shell });
  await settle();
  await removeAll(container);
  const said = dialog()!.textContent ?? "";
  expect(said).toContain("Could not confirm that everything was removed.");
  expect(said).not.toContain("could not be removed.");
  expect(said).toContain("Could not remove what the app added on this computer");
  expect(said).not.toContain("Everything was removed.");
  expect(dialog()!.querySelectorAll("ul")).toHaveLength(0);
  expect(said).not.toContain("the shell did not answer");
});

it("shows what was removed beside the parts that failed as a whole, a feature of the application and the daemon", async () => {
  const shell = shellWith([LOGIN, LINK, SHARING], {
    results: [{ change: LOGIN, outcome: { result: "cleared" } }],
    failed: [{ kind: "command_line", detail: "the link's state could not be read" }, DAEMON_DOWN],
  });
  const { container } = mount({ shell });
  await settle();
  await removeAll(container);
  const said = dialog()!.textContent ?? "";
  // No item is known to be left, and not everything is known to have gone.
  expect(said).toContain("Could not confirm that everything was removed.");
  expect(said).not.toContain("could not be removed.");
  expect(foldedTexts("1 removed")).toEqual(["RemovedLaunch at login"]);
  // An alert per part for what did not answer, which does not claim that nothing was removed.
  expect(said).toContain("Could not remove what the app added on this computer");
  expect(said).toContain("Could not remove what the daemon added");
  expect(said.match(/Some of its items may have been removed\. Open this again to see what is still in effect\./g)).toHaveLength(2);
  expect(said).not.toContain("the link's state could not be read");
  expect(said).not.toContain("The connection to the daemon was closed.");
});

it("says on the row that not everything could be read, and shows the daemon's failure in the dialog over the count of the rest, with Try again", async () => {
  const shell = shellWith([LOGIN], { listFailed: [DAEMON_DOWN] });
  const { container } = mount({ shell });
  await settle();
  expect(container.textContent).toContain("Could not read everything that is in effect.");
  // The alert itself is in the dialog, not on the page.
  expect(container.textContent).not.toContain("Could not read what the daemon added");
  expect(button(container, "Remove all")!.disabled).toBe(false);
  await open(container);
  const said = dialog()!.textContent ?? "";
  expect(said).toContain("Could not read what the daemon added");
  expect(said).not.toContain("Could not read what the app added on this computer");
  expect(said).not.toContain("The connection to the daemon was closed.");
  expect(foldedTexts("1 item will be removed")).toEqual(["Launch at login"]);
  // Something is listed, so the action is offered once its word is typed.
  typeWord();
  expect(button(dialog()!, "Remove all")!.disabled).toBe(false);
  // Try again asks the shell anew; the daemon answers now.
  shell.set([LOGIN, SHARING]);
  await click(button(dialog()!, "Try again"));
  await settle();
  expect(dialog()!.textContent).not.toContain("Could not read what the daemon added");
  expect(foldedTexts("2 items will be removed")).toHaveLength(2);
  expect(container.textContent).toContain("2 items in effect");
});

it("shows a feature of the application that could not tell over the count of the rest", async () => {
  const shell = shellWith([LOGIN, SHARING], { listFailed: [{ kind: "device_key", detail: "the saved daemons could not be read" }] });
  const { container } = mount({ shell });
  await settle();
  expect(container.textContent).toContain("Could not read everything that is in effect.");
  await open(container);
  const said = dialog()!.textContent ?? "";
  expect(said).toContain("Could not read what the app added on this computer");
  expect(said).not.toContain("Could not read what the daemon added");
  expect(foldedTexts("2 items will be removed")).toHaveLength(2);
  expect(said).not.toContain("the saved daemons could not be read");
});

it("offers no Confirm while the shell does not answer, only Try again", async () => {
  const shell = shellWith([LOGIN]);
  const { container } = mount({ shell });
  await settle();
  shell.list.mockRejectedValue(new Error("the shell did not answer"));
  await open(container);
  expect(dialog()!.textContent).toContain("Could not read what the app added on this computer");
  expect(dialog()!.textContent).not.toContain("will be removed");
  expect(dialog()!.querySelectorAll("ul")).toHaveLength(0);
  typeWord();
  expect(button(dialog()!, "Remove all")!.disabled).toBe(true);
  expect(button(dialog()!, "Try again")).toBeDefined();
});

it("counts nothing of a part whose read fails after a clearing, instead of what it listed before", async () => {
  const inUse: ShellSystemChangeResult = { change: SHARING, outcome: { result: "account_sharing_left", failure: { code: "account_in_use", params: { name: "Work", sessions: "1" }, message: "in use" } } };
  const shell = shellWith([LOGIN, SHARING], { results: [{ change: LOGIN, outcome: { result: "cleared" } }, inUse] });
  const { container } = mount({ shell });
  await settle();
  await open(container);
  typeWord();
  // The daemon answers the clearing and is gone for the read after it.
  shell.clearAll.mockImplementationOnce(async () => {
    shell.set([], [DAEMON_DOWN]);
    return { results: [{ change: LOGIN, outcome: { result: "cleared" } }, inUse], failed: [] };
  });
  await click(button(dialog()!, "Remove all"));
  await settle();
  // The account is in the result.
  expect(dialog()!.textContent).toContain("1 removed, 1 could not be removed.");
  expect(dialog()!.textContent).toContain("Work");
  await click(button(dialog()!, "Close"));
  expect(container.textContent).toContain("Could not read everything that is in effect.");
  expect(container.textContent).not.toContain("item in effect");
});

it("in a browser, shows nothing until the daemon has said what host it is on", async () => {
  const { container, daemon } = mount({ connected: false });
  await settle();
  expect(container.textContent).toBe("");
  await act(async () => daemon.store.setState({ host: { ...DAEMON_HOST, application: false } }));
  expect(container.textContent).toContain("Clean up before uninstalling");
});

it("in a browser on a daemon that runs on its own, the row says uninstalling it cleans up first and gives the uninstall command to copy, with no action", async () => {
  const { container, sent } = mount();
  await settle();
  const said = container.textContent ?? "";
  expect(said).toContain("Clean up before uninstalling");
  expect(said).toContain("from this server cleans up first. Run this on the server itself; it is not run from a browser.");
  expect(said).toContain("octoboardd uninstall");
  expect(said).not.toContain("in effect");
  // The copy button is the only control: nothing is listed, read or cleared from a browser.
  expect([...container.querySelectorAll("button")].map((candidate) => candidate.getAttribute("aria-label") ?? candidate.textContent)).toEqual(["Copy uninstall command"]);
  expect(sent).toHaveLength(0);
});

it("in a browser on a desktop application's own daemon, the row only says where the cleanup is done", async () => {
  const { container, sent } = mount({ applicationDaemon: true });
  await settle();
  expect(container.textContent).toContain("Clean up before uninstalling");
  expect(container.textContent).toContain("This cleanup is run where");
  expect(container.textContent).not.toContain("in effect");
  expect(container.textContent).not.toContain("octoboardd");
  expect(container.querySelectorAll("button")).toHaveLength(0);
  expect(sent).toHaveLength(0);
});

it("in a remote daemon's window, the row says to clean up in this computer's own window and asks the daemon nothing", async () => {
  const { container, sent } = mount({ kind: "tauri" });
  await settle();
  expect(container.textContent).toContain("Clean up before uninstalling");
  expect(container.textContent).toContain("This window shows a remote daemon.");
  expect(container.textContent).toContain("open Settings in its own window here");
  expect(container.textContent).toContain("A daemon on a server is cleaned up when it is uninstalled, on that server.");
  expect(container.textContent).not.toContain("octoboardd");
  expect(container.querySelectorAll("button")).toHaveLength(0);
  expect(sent).toHaveLength(0);
});

/** Puts the window in `language` for the rest of the test. */
function inLanguage(language: Language): void {
  const before = currentLanguage();
  act(() => setLanguage(language));
  onTestFinished(() => { act(() => setLanguage(before)); });
}

it("holds the row's action, with no status, until the shell has first answered", async () => {
  const shell = shellWith([LOGIN]);
  let answer!: (listed: ShellSystemChangesListed) => void;
  shell.list.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
  const { container } = mount({ shell });
  await settle();
  expect(button(container, "Remove all")!.disabled).toBe(true);
  expect(container.textContent).not.toContain("in effect");
  expect(container.textContent).not.toContain("Nothing to remove.");
  await act(async () => answer({ changes: [LOGIN], failed: [] }));
  expect(button(container, "Remove all")!.disabled).toBe(false);
  expect(container.textContent).toContain("1 item in effect");
});

it("opens the confirmation in the field the word is typed into, and moves focus into the result once confirmed", async () => {
  const { container } = mount({ shell: shellWith([LOGIN, LINK]) });
  await settle();
  await open(container);
  const field = dialog()!.querySelector("input")!;
  await vi.waitFor(() => expect(document.activeElement).toBe(field));
  typeWord();
  await click(button(dialog()!, "Remove all"));
  await settle();
  expect(dialog()!.textContent).toContain("Result of the cleanup");
  // On the result's first line, which is read with the dialog's title; never behind it on the section, as nothing is
  // left once the row is read again.
  await vi.waitFor(() => expect(document.activeElement?.textContent).toBe("Everything was removed."));
  expect(dialog()!.contains(document.activeElement)).toBe(true);
});

it("puts focus back on the row's action when the result closes and something is left", async () => {
  const loginLeft: ShellSystemChangeResult = { change: LOGIN, outcome: { result: "launch_at_login_left", detail: "denied" } };
  const { container } = mount({ shell: shellWith([LOGIN, LINK], { results: [loginLeft, { change: LINK, outcome: { result: "cleared" } }] }) });
  await settle();
  await removeAll(container);
  await click(button(dialog()!, "Close"));
  expect(dialog()).toBeNull();
  const action = button(container, "Remove all")!;
  expect(action.disabled).toBe(false);
  await vi.waitFor(() => expect(document.activeElement).toBe(action));
});

it("puts focus on the section's entry when the result closes with nothing left, the row's action being disabled", async () => {
  const { container, entry } = mount({ shell: shellWith([LOGIN, LINK]) });
  await settle();
  await removeAll(container);
  await click(button(dialog()!, "Close"));
  expect(button(container, "Remove all")!.disabled).toBe(true);
  await vi.waitFor(() => expect(document.activeElement).toBe(entry));
});

it.each([
  ["the row's action, once the read finds an item left", [LOGIN], "action"],
  ["the section's entry, once the read finds nothing left", [], "entry"],
] as const)("waits, when a mixed result closes while the row's read is still out, and then puts focus on %s", async (_, after, target) => {
  const loginLeft: ShellSystemChangeResult = { change: LOGIN, outcome: { result: "launch_at_login_left", detail: "denied" } };
  const shell = shellWith([LOGIN, LINK], { results: [loginLeft, { change: LINK, outcome: { result: "cleared" } }] });
  const { container, entry } = mount({ shell });
  await settle();
  await open(container);
  // The read after the clearing is slow to answer: still out when the result closes.
  let answer!: () => void;
  shell.list.mockImplementationOnce(() => new Promise<ShellSystemChangesListed>((resolve) => (answer = () => resolve({ changes: [...after], failed: [] }))));
  typeWord();
  await click(button(dialog()!, "Remove all"));
  await settle();
  expect(dialog()!.textContent).toContain("1 removed, 1 could not be removed.");
  await click(button(dialog()!, "Close"));
  expect(dialog()).toBeNull();
  // Nothing is chosen on what the row showed before the clearing.
  await act(async () => void (await new Promise((done) => setTimeout(done, 100))));
  expect(document.activeElement).toBe(document.body);
  await act(async () => answer());
  await settle();
  const action = button(container, "Remove all")!;
  if (target === "action") {
    expect(action.disabled).toBe(false);
    await vi.waitFor(() => expect(document.activeElement).toBe(action));
  } else {
    expect(action.disabled).toBe(true);
    await vi.waitFor(() => expect(document.activeElement).toBe(entry));
  }
});

it("puts focus on the result's first line, read with the dialog's title, and says it nowhere else", async () => {
  const loginLeft: ShellSystemChangeResult = { change: LOGIN, outcome: { result: "launch_at_login_left", detail: "denied" } };
  const { container } = mount({ shell: shellWith([LOGIN, LINK], { results: [loginLeft, { change: LINK, outcome: { result: "cleared" } }] }) });
  await settle();
  await removeAll(container);
  const result = dialog()!;
  const line = () => [...result.querySelectorAll("p")].find((candidate) => candidate.textContent === "1 removed, 1 could not be removed.")!;
  await vi.waitFor(() => expect(document.activeElement).toBe(line()));
  // A place focus is put, not a tab stop, and in reach of a screen reader.
  expect(line().tabIndex).toBe(-1);
  expect(line().closest('[aria-hidden="true"]')).toBeNull();
  // Said once, by the focus move: no description, no status and no alert repeat it.
  expect(result.hasAttribute("aria-describedby")).toBe(false);
  expect([...result.querySelectorAll('[role="status"], [role="alert"]')].filter((region) => region.textContent !== "")).toHaveLength(0);
  expect([...result.querySelectorAll("*")].filter((element) => element.textContent === "1 removed, 1 could not be removed." && element.closest('[aria-hidden="true"]') === null && ![...element.children].some((child) => child.textContent === element.textContent))).toHaveLength(1);
});

it("waits for the window to be attended before putting focus on the result's line, and puts it there anew on each return", async () => {
  const attention = windowAttention();
  const shell = shellWith([LOGIN, LINK]);
  // The clearing raises a system prompt, which takes the window's focus until it is answered or cancelled.
  let answer!: () => void;
  shell.clearAll.mockImplementationOnce(async () => {
    await new Promise<void>((resolve) => (answer = resolve));
    return { results: [{ change: LOGIN, outcome: { result: "cleared" } }, { change: LINK, outcome: { result: "command_line_left", error: { code: "refused", path: "/usr/local/bin/octoboardd", detail: "User canceled" } } }], failed: [] };
  });
  const { container } = mount({ shell });
  await settle();
  await open(container);
  typeWord();
  await click(button(dialog()!, "Remove all"));
  attention(false);
  await act(async () => answer());
  await settle();
  const result = dialog()!;
  expect(result.textContent).toContain("Result of the cleanup");
  const line = result.querySelector<HTMLElement>("p[tabindex='-1']")!;
  expect(line.textContent).toBe("1 removed, 1 could not be removed.");
  const landings: string[] = [];
  result.addEventListener("focusin", (event) => landings.push(event.target === line ? "line" : "elsewhere"));
  expect(document.activeElement).not.toBe(line);
  // Back in the window: focus moves onto the line, once.
  attention(true);
  await vi.waitFor(() => expect(document.activeElement).toBe(line));
  expect(landings.filter((landing) => landing === "line")).toHaveLength(1);
  // Away and back again while the result is open: the line takes focus anew, through the hidden relay.
  attention(false);
  attention(true);
  await vi.waitFor(() => expect(landings.filter((landing) => landing === "line")).toHaveLength(2));
  expect(document.activeElement).toBe(line);
});

it("does not take Confirm while the read the dialog opens with is pending, even with the word typed", async () => {
  const shell = shellWith([LOGIN]);
  const { container } = mount({ shell });
  await settle();
  let answer!: (listed: ShellSystemChangesListed) => void;
  shell.list.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
  await click(button(container, "Remove all"));
  typeWord();
  const submit = () => act(async () => void dialog()!.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  await submit();
  await settle();
  expect(shell.clearAll).not.toHaveBeenCalled();
  await act(async () => answer({ changes: [LOGIN], failed: [] }));
  await settle();
  await submit();
  await settle();
  expect(shell.clearAll).toHaveBeenCalledTimes(1);
});

it("says, with the daemon unread, that Remove all still clears what it added, and what that does to sharing and idle sessions, and clears it on confirming", async () => {
  const shell = shellWith([LOGIN], { listFailed: [DAEMON_DOWN] });
  const { container } = mount({ shell });
  await settle();
  await open(container);
  const said = dialog()!.textContent ?? "";
  expect(said).toContain("These items are not listed below, and Remove all still tries to remove them.");
  expect(said).toContain("An account that is sharing stops sharing, and its idle sessions are restarted; their conversations continue.");
  expect(said).toContain("The app’s own data and the conversation history that sharing moved are kept.");
  // Said once, not as a mark on a line: no line stands for the daemon's accounts.
  expect(said.match(/idle sessions/g)).toHaveLength(1);
  expect(folded("1 item will be removed").flatMap((line) => [...line.querySelectorAll("button")])).toHaveLength(0);
  typeWord();
  await click(button(dialog()!, "Remove all"));
  await settle();
  expect(shell.clearAll).toHaveBeenCalledTimes(1);
});

it("says nothing of clearing what could not be read where nothing is listed, as Confirm is not offered", async () => {
  const shell = shellWith([LOGIN]);
  const { container } = mount({ shell });
  await settle();
  shell.list.mockRejectedValue(new Error("the shell did not answer"));
  await open(container);
  expect(dialog()!.textContent).not.toContain("still tries to remove them");
  expect(dialog()!.textContent).toContain("The app’s own data is kept.");
});

it("offers one Try again for both parts that could not be read, on the last alert", async () => {
  const shell = shellWith([LOGIN], { listFailed: [{ kind: "device_key", detail: "unreadable" }, DAEMON_DOWN] });
  const { container } = mount({ shell });
  await settle();
  await open(container);
  expect(dialog()!.textContent).toContain("Could not read what the app added on this computer");
  expect(dialog()!.textContent).toContain("Could not read what the daemon added");
  const retries = [...dialog()!.querySelectorAll("button")].filter((candidate) => candidate.textContent === "Try again");
  expect(retries).toHaveLength(1);
  // On the daemon's alert, the last: after both titles, the daemon's coming after the app's.
  const title = (text: string) => [...dialog()!.querySelectorAll("*")].find((element) => element.children.length === 0 && element.textContent === text)!;
  const app = title("Could not read what the app added on this computer");
  const daemon = title("Could not read what the daemon added");
  expect(app.compareDocumentPosition(daemon) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(daemon.compareDocumentPosition(retries[0]!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("hands focus from a Try again whose alert went to the one that took its place, and to the count once both went", async () => {
  const shell = shellWith([LOGIN], { listFailed: [{ kind: "device_key", detail: "unreadable" }, DAEMON_DOWN] });
  const { container } = mount({ shell });
  await settle();
  await open(container);
  const retry = () => button(dialog()!, "Try again")!;
  // The daemon answers now; the app's feature still does not.
  shell.set([LOGIN, SHARING], [{ kind: "device_key", detail: "unreadable" }]);
  act(() => retry().focus());
  const first = retry();
  await click(first);
  await settle();
  expect(dialog()!.textContent).not.toContain("Could not read what the daemon added");
  expect(first.isConnected).toBe(false);
  expect(document.activeElement).toBe(retry());
  // Everything answers.
  shell.set([LOGIN, SHARING]);
  await click(retry());
  await settle();
  expect(button(dialog()!, "Try again")).toBeUndefined();
  expect(document.activeElement).toBe(trigger(dialog()!, "2 items will be removed"));
});

it("closes the dialog when Try again finds nothing left, and puts focus on the section's entry", async () => {
  const shell = shellWith([LOGIN], { listFailed: [DAEMON_DOWN] });
  const { container, entry } = mount({ shell });
  await settle();
  await open(container);
  shell.set([]);
  await click(button(dialog()!, "Try again"));
  await settle();
  expect(dialog()).toBeNull();
  expect(container.textContent).toContain("Nothing to remove.");
  await vi.waitFor(() => expect(document.activeElement).toBe(entry));
});

it("does not announce a failure the dialog opens with, and announces one Try again leaves, after the focus delay and for each attempt", async () => {
  fakeTimers();
  const shell = shellWith([LOGIN], { listFailed: [DAEMON_DOWN] });
  const { container } = mount({ shell });
  await settle();
  await open(container);
  const alert = () => document.body.querySelector('[role="alert"]');
  await act(async () => void (await vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS)));
  expect(dialog()!.textContent).toContain("Could not read what the daemon added");
  expect(alert()).toBeNull();
  // Try again, and the daemon still does not answer: the same words, spoken again once focus has settled.
  await click(button(dialog()!, "Try again"));
  await settle();
  expect(alert()).toBeNull();
  await act(async () => void (await vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS)));
  const first = alert();
  expect(first?.textContent).toContain("Could not read what the daemon added");
  expect(first?.textContent).toContain("still tries to remove them");
  // Once more: a new announcement, not the old one left in place.
  await click(button(dialog()!, "Try again"));
  await settle();
  await act(async () => void (await vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS)));
  expect(alert()).not.toBeNull();
  expect(alert()).not.toBe(first);
});

it("puts a failure Try again leaves in the warning modal once per attempt, hidden from a screen reader until its alert", async () => {
  fakeTimers();
  const shell = shellWith([LOGIN], { listFailed: [DAEMON_DOWN] });
  const { container } = mount({ shell });
  await settle();
  await open(container);
  await act(async () => void (await vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS)));
  const warning = dialog()!;
  // The warning modal is an alert dialog, which speaks what is put into it as well as any alert inside it.
  expect(warning.getAttribute("role")).toBe("alertdialog");
  const title = "Could not read what the daemon added";
  /** The failure's words that a screen reader can reach, and what of them is an alert. */
  const exposed = () => [...warning.querySelectorAll("*")].filter((element) => element.textContent?.includes(title) && ![...element.children].some((child) => child.textContent?.includes(title)) && element.closest('[aria-hidden="true"]') === null);
  const alerts = () => [...warning.querySelectorAll('[role="alert"]')];
  // Opened with: part of what the dialog says, and not announced.
  expect(exposed()).toHaveLength(1);
  expect(alerts()).toHaveLength(0);
  for (let attempt = 1; attempt <= 2; attempt += 1) {
    await click(button(warning, "Try again"));
    await settle();
    // Put back in place while its announcement waits, out of a screen reader's reach, so the dialog does not read it.
    expect(warning.textContent).toContain(title);
    expect(exposed()).toHaveLength(0);
    expect(alerts()).toHaveLength(0);
    await act(async () => void (await vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS)));
    // Then once, as the one alert, its title and summary only.
    expect(exposed()).toHaveLength(1);
    expect(alerts()).toHaveLength(1);
    expect(alerts()[0].textContent).toContain(title);
    expect(alerts()[0].textContent).toContain("still tries to remove them");
    expect(alerts()[0].textContent).not.toContain("Try again");
  }
});

it("keeps an account's name whole and in its own direction beside its agent, the two following the interface's direction", async () => {
  inLanguage("ar");
  const { container } = mount({ shell: shellWith([SHARING]) });
  await settle();
  await click(button(container, "إزالة الكل"));
  await settle();
  const line = dialog()!.querySelector(".disclosure li")!;
  const values = [...line.querySelectorAll("bdi")].map((value) => value.textContent);
  expect(values).toEqual(["Claude Code", "Work"]);
  // The agent and the name are values of their own, not one composite run in one direction.
  expect(line.querySelector('[dir="auto"], [dir="ltr"]')).toBeNull();
});

it("tells two keys for the same user and host apart by their saved daemons' names", async () => {
  const sameAddress: ShellSystemChange = { ...KEY, id: "e9", name: "Build server, port 2222" };
  const { container } = mount({ shell: shellWith([KEY, sameAddress, OTHER_KEY]) });
  await settle();
  await open(container);
  const [first, second, third] = foldedTexts("3 items will be removed");
  expect(first).toContain("dev@build.example");
  expect(first).toContain("Build server");
  expect(second).toContain("Build server, port 2222");
  // A key whose address is its own needs no name.
  expect(third).not.toContain("Staging");
});

it("compares the typed word in the window's language, so Turkish capitals of a lowercase word are accepted", async () => {
  inLanguage("tr");
  const shell = shellWith([LOGIN]);
  const { container } = mount({ shell });
  await settle();
  await click(button(container, "Tümünü kaldır"));
  await settle();
  const confirm = () => button(dialog()!, "Tümünü kaldır")!;
  expect(confirm().disabled).toBe(true);
  dom.type(dialog()!.querySelector("input")!, "kaldır");
  expect(confirm().disabled).toBe(false);
});

it("does not open the confirmation again when it is cancelled while its read is still out", async () => {
  const shell = shellWith([LOGIN]);
  const { container } = mount({ shell });
  await settle();
  let answer!: (listed: ShellSystemChangesListed) => void;
  shell.list.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
  await click(button(container, "Remove all"));
  expect(dialog()).not.toBeNull();
  await click(button(dialog()!, "Cancel"));
  expect(dialog()).toBeNull();
  await act(async () => answer({ changes: [LOGIN, LINK], failed: [] }));
  await settle();
  expect(dialog()).toBeNull();
  // The row still follows what the read found.
  expect(container.textContent).toContain("2 items in effect");
});

it("says a listed item could not be removed only where one was left, a part that failed whole leaving it unknown", async () => {
  const loginLeft: ShellSystemChangeResult = { change: LOGIN, outcome: { result: "launch_at_login_left", detail: "denied" } };
  const { container } = mount({ shell: shellWith([LOGIN, SHARING], { results: [loginLeft], failed: [DAEMON_DOWN] }) });
  await settle();
  await removeAll(container);
  const said = dialog()!.textContent ?? "";
  expect(said).toContain("1 item could not be removed.");
  expect(said).not.toContain("Could not confirm that everything was removed.");
  expect(said).toContain("Could not remove what the daemon added");
});
