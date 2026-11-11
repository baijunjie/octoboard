// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { FIRST_WRITE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { booleanSetting } from "../configuration/booleanSettings";
import { translatorFor } from "../i18n/catalog";
import { currentLanguage, setLanguage } from "../i18n/language";
import { LanguageProvider } from "../i18n/react";
import { ThemeProvider } from "../preferences/theme";
import { UpdatesProvider } from "../updates/UpdatesProvider";
import type { LaunchAtLoginCapability, MacosPermissionsCapability, PlatformAdapter } from "../platform";
import type { Event } from "../protocol";
import type { State } from "../store";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { ALL_COMMANDS, FIND_BINDING } from "../shortcuts/commands";
import { framesAsTimers, settleFrames } from "../components/dialog/dialogTestSupport";
import { eventKey } from "../shortcuts/binding";
import { fakeTimers, flushPromises } from "../test/clock";
import { click, type } from "../test/dom";
import { BROWSER_HOST, MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { matchesWidthQuery } from "../test/viewport";
import { SettingsDialog } from "./SettingsDialog";
import type { SettingsNavigation, SettingsSectionId } from "./sections";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The list is a chunk of its own, opened only from a button.
vi.mock("../about/ThirdPartyLicensesDialog", () => ({ ThirdPartyLicensesDialog: () => null }));

const t = translatorFor("en");
const BROWSER: PlatformAdapter = { kind: "browser", host: BROWSER_HOST };

/** The window is `width` wide, so the sections are a column beside the panel (wide) or a row under the field (narrow). */
function sizeWindow(width: number) {
  document.documentElement.style.setProperty("--breakpoint-tablet", "768px");
  document.documentElement.style.setProperty("--breakpoint-phone", "390px");
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: matchesWidthQuery(media, width), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--breakpoint-tablet");
    document.documentElement.style.removeProperty("--breakpoint-phone");
  });
}

beforeEach(() => sizeWindow(1440));

interface Mounted {
  onClose: ReturnType<typeof vi.fn>;
  show: (navigation: SettingsNavigation) => Promise<void>;
}

async function open({ section, state = {}, navigation, platform = BROWSER }: { section?: SettingsSectionId; state?: Partial<State>; navigation?: SettingsNavigation; platform?: PlatformAdapter } = {}): Promise<Mounted> {
  const onClose = vi.fn();
  const daemon = testDaemon({ state: { connectionState: "open", settings: DEFAULT_SETTINGS, ...state }, request: async (): Promise<Event> => ({ type: "ack", id: null }) });
  const dialog = (nav?: SettingsNavigation) => (
    <LanguageProvider>
      <ThemeProvider>
        <UpdatesProvider firstCheckDelayMs={null}>
          <SettingsDialog {...(section ? { initialSection: section } : {})} {...(nav ? { navigation: nav } : {})} onClose={onClose} />
        </UpdatesProvider>
      </ThemeProvider>
    </LanguageProvider>
  );
  const host = mountHost(dialog(navigation), { platform, daemon });
  for (let turn = 0; turn < 3; turn += 1) await act(flushPromises);
  return { onClose, show: async (next) => { host.render(dialog(next)); for (let turn = 0; turn < 3; turn += 1) await act(flushPromises); } };
}

const field = () => document.querySelector<HTMLInputElement>('input[aria-label="Search settings"]')!;
const tabs = () => [...document.querySelectorAll<HTMLElement>('[role="tab"]')];
// While nothing matches, the tabs are hidden rather than taken away.
const shownTabs = () => tabs().filter((tab) => tab.closest("[hidden]") === null);
const tabNames = () => tabs().map((tab) => tab.getAttribute("aria-label") ?? tab.textContent);
const selectedTab = () => document.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
const panel = () => document.querySelector<HTMLElement>('[role="tabpanel"]');
const entries = () => [...(panel()?.querySelectorAll<HTMLElement>("[data-settings-search-id]") ?? [])];
const shownEntries = () => entries().filter((element) => element.closest("[hidden]") === null).map((element) => element.dataset.settingsSearchId);
const status = () => document.querySelector('[role="status"]')?.textContent;
const search = async (query: string) => { type(field(), query); await act(flushPromises); };
const escape = (target: Element) => act(async () => void target.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
const clearButton = () => document.querySelector<HTMLElement>("[data-field-clear]")!;
const indicators = () => document.querySelectorAll('[role="tablist"] [data-slot="tabs-indicator"]');
/** The decorative ground behind the wide section column, whose end edge is the divider from the panel. */
const grounds = () => [...document.querySelectorAll<HTMLElement>('[aria-hidden="true"].bg-surface-secondary')];

describe("the search field", () => {
  it("is named Search settings and sits before the section tabs in the Tab order", async () => {
    await open();
    const order = field().compareDocumentPosition(document.querySelector('[role="tablist"]')!);
    expect(order & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("sits above the tab row when the window is narrow", async () => {
    sizeWindow(500);
    await open();
    const list = document.querySelector('[role="tablist"]')!;
    expect(list.getAttribute("aria-orientation")).toBe("horizontal");
    expect(field().compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("leaves the opening focus on the selected section's tab", async () => {
    await open({ section: "git" });
    expect([...document.querySelectorAll("[data-opening-focus]")]).toEqual([selectedTab()]);
    expect(selectedTab()?.textContent).toContain("Git");
    expect(document.activeElement).toBe(selectedTab());
  });

  it("lists every section, without counts, while nothing is searched for", async () => {
    await open();
    expect(tabNames().every((name) => !/matching/.test(name ?? ""))).toBe(true);
    expect(tabs().length).toBeGreaterThan(5);
    expect(status()).toBe("");
  });

  it("filters nothing for a blank query", async () => {
    await open({ section: "general" });
    const before = shownEntries();
    await search("   ");
    expect(shownEntries()).toEqual(before);
    expect(tabs().length).toBeGreaterThan(5);
  });
});

describe("while a query is entered", () => {
  it("lists only the sections with a match, each with how many, and shows only the matching rows of the selected one", async () => {
    await open({ section: "general" });
    const all = shownEntries();
    await search("sorting");
    expect(tabNames()).toEqual(["General, 3 matching settings"]);
    expect(shownEntries()).toEqual(["session-order.sessions", "session-order.activeProjects", "session-order.inactiveProjects"]);
    // The others are hidden, not taken out.
    expect(entries().map((element) => element.dataset.settingsSearchId)).toEqual(all);
    expect(panel()!.querySelector('[data-settings-search-id="appearance"]')!.closest("[hidden]")).not.toBeNull();
  });

  it("words a single match in the singular", async () => {
    await open({ section: "general" });
    await search("markdown");
    expect(tabNames()).toEqual(["General, 1 matching setting"]);
    expect(shownEntries()).toEqual(["markdown-style"]);
  });

  it("matches in any case and ignores the space around the query", async () => {
    await open({ section: "general" });
    await search("  MARKDOWN  ");
    expect(shownEntries()).toEqual(["markdown-style"]);
  });

  it("needs every word, wherever each is found in the entry's text", async () => {
    await open({ section: "general" });
    await search("general sorting");
    expect(shownEntries()).toHaveLength(3);
    await search("appearance sorting");
    expect(shownTabs()).toHaveLength(0);
  });

  it("counts the sections that are not on screen, and lists those that have a match", async () => {
    await open({ section: "general" });
    await search("squash commits");
    // Git is not the selected section, and its panel is not rendered, yet its tab has its count.
    expect(tabNames()).toEqual(["Git, 1 matching setting"]);
  });

  it("hides a group with no matching row, heading and all", async () => {
    await open({ section: "git" });
    await search("revert gate");
    const groups = [...panel()!.querySelectorAll<HTMLElement>("[data-settings-search-group]")];
    // The group element stays shown, so that its rows can be seen leaving; its heading goes with them.
    const visible = groups.filter((group) => !group.querySelector("h3")!.closest("[hidden]")).map((group) => group.dataset.settingsSearchGroup);
    expect(visible).toEqual(["settings.git.group.protection"]);
    expect(groups.length).toBeGreaterThan(1);
    expect(shownEntries()).toEqual(["git_workflow.revert_gate"]);
  });

  it("finds a row by its group's heading", async () => {
    await open({ section: "git" });
    await search("repository");
    expect(shownEntries()).toEqual(["clone-dir", "auto_pull_repositories"]);
  });

  it("shows the first section with a match when the selected one has none, and keeps it once the query is cleared", async () => {
    await open({ section: "about" });
    await search("sorting");
    expect(selectedTab()?.textContent).toContain("General");
    expect(shownEntries()).toHaveLength(3);
    await click(clearButton());
    expect(field().value).toBe("");
    expect(selectedTab()?.textContent).toContain("General");
    expect(tabs().length).toBeGreaterThan(5);
    expect(shownEntries()).toContain("appearance");
  });

  it("keeps the selected section when it has a match", async () => {
    await open({ section: "git" });
    await search("pull");
    expect(selectedTab()?.textContent).toContain("Git");
  });

  it("keeps the section on screen across a query nothing matches, one both it and another match, and a clear", async () => {
    await open({ section: "git" });
    await search("sorting");
    expect(selectedTab()?.textContent).toContain("General");
    await search("zzzznope");
    await search("e");
    // Git has a match for this query too, which would take the section back to it had General not been kept.
    expect(tabNames().some((name) => name?.startsWith("Git,"))).toBe(true);
    expect(selectedTab()?.textContent).toContain("General");
    await search("");
    expect(selectedTab()?.textContent).toContain("General");
  });

  it("keeps the section on screen, and its panel mounted, when a language switch finds nothing and the search is matched again", async () => {
    framesAsTimers();
    const previous = currentLanguage();
    onTestFinished(() => { act(() => setLanguage(previous)); });
    await open({ section: "about" });
    await search("language");
    expect(selectedTab()?.textContent).toContain("General");
    const shown = panel()!;
    const row = shown.querySelector('[data-settings-search-id="language"]')!;
    act(() => setLanguage("de"));
    await settleFrames();
    expect(shownTabs()).toEqual([]);
    act(() => setLanguage("en"));
    await settleFrames();
    expect(selectedTab()?.textContent).toContain("General");
    expect(panel()).toBe(shown);
    expect(shown.querySelector('[data-settings-search-id="language"]')).toBe(row);
    act(() => setLanguage("de"));
    await settleFrames();
    await click(clearButton());
    expect(selectedTab()?.textContent).toContain("Allgemein");
    expect(panel()).toBe(shown);
  });

  it("keeps a half-typed value of a hidden row, which comes back with the row", async () => {
    await open({ section: "git" });
    const input = () => panel()!.querySelector<HTMLInputElement>('[data-settings-search-id="clone-dir"] input')!;
    const original = input();
    type(original, "/draft/directory");
    await search("revert gate");
    expect(input().closest("[hidden]")).not.toBeNull();
    await search("");
    expect(input()).toBe(original);
    expect(input().value).toBe("/draft/directory");
  });
});

describe("a search that finds nothing", () => {
  it("replaces the tabs and the panel with an empty state and announces it politely", async () => {
    fakeTimers();
    await open({ section: "general" });
    await act(async () => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
    await search("zzzznope");
    expect(document.querySelector('[role="tablist"]')!.closest("[hidden]")).not.toBeNull();
    expect(panel()!.hidden).toBe(true);
    expect(document.body.textContent).toContain("No matching settings");
    expect(document.querySelector('[role="status"]')?.textContent).toBe("No setting matches the search.");
  });

  it("keeps the column's decorative ground, divider included, while nothing is found", async () => {
    await open({ section: "general" });
    expect(grounds()).toHaveLength(1);
    await search("zzzznope");
    expect(document.body.textContent).toContain("No matching settings");
    expect(grounds()).toHaveLength(1);
    expect(grounds()[0]!.textContent).toBe("");
  });

  it("keeps what was typed in the section on screen, hidden with it, for when the query finds it again", async () => {
    await open({ section: "git" });
    const input = () => panel()!.querySelector<HTMLInputElement>('[data-settings-search-id="clone-dir"] input')!;
    const original = input();
    type(original, "/draft/directory");
    await search("clonx");
    expect(panel()!.hidden).toBe(true);
    await search("clone");
    expect(panel()!.hidden).toBe(false);
    expect(input()).toBe(original);
    expect(input().value).toBe("/draft/directory");
    expect(input().closest("[hidden]")).toBeNull();
  });

  it("brings the sections back once the query is cleared", async () => {
    await open({ section: "general" });
    await search("zzzznope");
    await click(clearButton());
    expect(tabs().length).toBeGreaterThan(5);
    expect(document.body.textContent).not.toContain("No matching settings");
    expect(selectedTab()?.textContent).toContain("General");
  });

  it("announces how many settings match when some do", async () => {
    fakeTimers();
    await open({ section: "general" });
    await act(async () => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
    await search("sorting");
    expect(status()).toBe("3 matching settings");
    await search("markdown");
    expect(status()).toBe("1 matching setting");
  });
});

describe("the section strip while a query is entered", () => {
  it("keeps the narrow strip's indicator mounted but unseen, and has the selected tab draw the fill itself", async () => {
    sizeWindow(500);
    await open({ section: "general" });
    expect(indicators()).toHaveLength(1);
    expect(selectedTab()!.className).not.toContain("ui-selected:bg-segment");
    expect(indicators()[0]!.className).not.toContain("opacity-0");
    await search("sorting");
    expect(indicators()).toHaveLength(1);
    expect(selectedTab()!.querySelector('[data-slot="tabs-indicator"]')!.className).toContain("opacity-0");
    expect(selectedTab()!.className).toContain("ui-selected:bg-segment");
    await click(clearButton());
    expect(indicators()).toHaveLength(1);
    expect(selectedTab()!.className).not.toContain("ui-selected:bg-segment");
    expect(selectedTab()!.querySelector('[data-slot="tabs-indicator"]')!.className).not.toContain("opacity-0");
  });

  it("gives the wide menu no indicator while searching, and its selected row keeps its own fill", async () => {
    await open({ section: "general" });
    expect(selectedTab()!.className).toContain("ui-selected:bg-(--menu-raised)");
    await search("sorting");
    expect(indicators()).toHaveLength(0);
    expect(selectedTab()!.className).toContain("ui-selected:bg-(--menu-raised)");
    expect(selectedTab()!.className).not.toContain("ui-selected:bg-segment");
    await click(clearButton());
    expect(indicators()).toHaveLength(0);
    expect(selectedTab()!.className).toContain("ui-selected:bg-(--menu-raised)");
  });
});

describe("clearing the field", () => {
  it("clears with the clear button", async () => {
    await open({ section: "general" });
    await search("sorting");
    await click(clearButton());
    expect(field().value).toBe("");
    expect(shownEntries()).toContain("appearance");
  });

  it("clears on Escape, without closing the dialog", async () => {
    const { onClose } = await open({ section: "general" });
    await search("sorting");
    field().focus();
    await escape(field());
    expect(field().value).toBe("");
    expect(onClose).not.toHaveBeenCalled();
    expect(tabs().length).toBeGreaterThan(5);
  });

  it("closes the dialog on Escape once the field is empty", async () => {
    const { onClose } = await open({ section: "general" });
    field().focus();
    await escape(field());
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("needs a second Escape to close after the first has cleared the field", async () => {
    const { onClose } = await open({ section: "general" });
    await search("sorting");
    field().focus();
    await escape(field());
    expect(onClose).not.toHaveBeenCalled();
    await escape(field());
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe("a navigation request", () => {
  it("clears the query while searching, so the requested section is shown whole", async () => {
    const { show } = await open({ navigation: { nonce: 1, section: "general", addAgent: false } });
    await search("sorting");
    expect(shownEntries()).toHaveLength(3);
    await show({ nonce: 2, section: "git", addAgent: false });
    expect(field().value).toBe("");
    expect(selectedTab()?.textContent).toContain("Git");
    expect(tabs().length).toBeGreaterThan(5);
    expect(shownEntries()).toEqual(entries().map((element) => element.dataset.settingsSearchId));
    expect(panel()!.querySelectorAll("[hidden]").length).toBe(0);
  });

  it("clears the query even when the section it asks for is the one shown", async () => {
    const { show } = await open({ navigation: { nonce: 1, section: "general", addAgent: false } });
    await search("sorting");
    await show({ nonce: 2, section: "general", addAgent: false });
    expect(field().value).toBe("");
    expect(shownEntries()).toContain("appearance");
  });

  it("keeps the section on screen when the request names none", async () => {
    const { show } = await open({ navigation: { nonce: 1, section: "about", addAgent: false } });
    await search("sorting");
    expect(selectedTab()?.textContent).toContain("General");
    await show({ nonce: 2, section: undefined, addAgent: false });
    expect(field().value).toBe("");
    expect(selectedTab()?.textContent).toContain("General");
  });

  it("moves focus to the requested section's tab", async () => {
    const { show } = await open({ navigation: { nonce: 1, section: "general", addAgent: false } });
    await search("sorting");
    await show({ nonce: 2, section: "git", addAgent: false });
    await act(flushPromises);
    expect(document.activeElement).toBe(selectedTab());
  });
});

describe("what is found by name only or as a whole", () => {
  it("finds the keyboard shortcuts one by one, by their command's name", async () => {
    const command = ALL_COMMANDS.find((candidate) => candidate.availability === "always")!;
    await open({ section: "shortcuts" });
    await search(t(command.label));
    expect(shownEntries()).toContain(`shortcut.${command.id}`);
    expect(shownEntries().length).toBeLessThan(entries().length);
  });

  it("finds the notifications row by its name", async () => {
    await open({ section: "general" });
    await search("waiting for you");
    expect(tabNames()).toEqual(["Notifications, 1 matching setting"]);
  });

  it("finds the agent accounts as one block, by Agents and by the section's name", async () => {
    await open({ section: "accounts" });
    await search("agents");
    expect(shownEntries()).toEqual(["accounts.agents"]);
    await search("agent accounts");
    expect(shownEntries()).toContain("accounts.agents");
  });

  it("hides the agent accounts block from a search for one of the section's switches", async () => {
    await open({ section: "accounts" });
    await search(t(booleanSetting("auto_select_account").label));
    const block = panel()!.querySelector<HTMLElement>('[data-settings-search-id="accounts.agents"]')!;
    expect(block.closest("[hidden]")).not.toBeNull();
    expect(shownEntries()).toEqual(["auto_select_account"]);
  });

  it("finds the trusted folders as one block, by the section's name, and does not search the folders", async () => {
    await open({ section: "trusted-folders", state: { trustedDirectories: ["/Users/someone/zzfolder"] } });
    await search("trusted");
    expect(shownEntries()).toEqual(["trusted-folders.list"]);
    await search("zzfolder");
    expect(shownTabs()).toHaveLength(0);
    expect(document.body.textContent).toContain("No matching settings");
  });

  it("finds the trusted folders block by the section's intro", async () => {
    await open({ section: "general" });
    await search("trust everything");
    expect(tabNames()).toEqual(["Trusted folders, 1 matching setting"]);
  });
});

describe("the Find shortcut", () => {
  const HOSTS = [["macOS", MAC_HOST], ["Windows", WINDOWS_HOST]] as const;
  const FIND = { key: "f", ...eventKey(FIND_BINDING, BROWSER_HOST) };
  const onHost = (host: typeof MAC_HOST) => ({ key: "f", ...eventKey(FIND_BINDING, host) });
  /** Presses `init` on `target` and returns the event, to see whether it was taken. */
  const press = (target: Element, init: KeyboardEventInit = FIND) => {
    const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
    act(() => void target.dispatchEvent(event));
    return event;
  };
  const selection = (input: HTMLInputElement) => [input.selectionStart, input.selectionEnd];

  describe.each(HOSTS)("on %s", (_name, host) => {
    const platform: PlatformAdapter = { kind: "browser", host };

    it("moves focus from the section tab to the search field", async () => {
      await open({ section: "general", platform });
      expect(document.activeElement).toBe(selectedTab());
      const event = press(document.activeElement!, onHost(host));
      expect(document.activeElement).toBe(field());
      expect(event.defaultPrevented).toBe(true);
    });

    it("is the platform's modifier that does it, not the other one", async () => {
      await open({ section: "general", platform });
      const other = onHost(host === MAC_HOST ? WINDOWS_HOST : MAC_HOST);
      const event = press(selectedTab()!, other);
      expect(document.activeElement).toBe(selectedTab());
      expect(event.defaultPrevented).toBe(false);
    });
  });

  it("moves focus from a control in a row to the search field", async () => {
    await open({ section: "git" });
    const input = panel()!.querySelector<HTMLInputElement>('[data-settings-search-id="clone-dir"] input')!;
    input.focus();
    expect(document.activeElement).toBe(input);
    press(input);
    expect(document.activeElement).toBe(field());
  });

  it("selects the text already in the field", async () => {
    await open({ section: "general" });
    await search("sorting");
    selectedTab()!.focus();
    press(selectedTab()!);
    expect(document.activeElement).toBe(field());
    expect(field().value).toBe("sorting");
    expect(selection(field())).toEqual([0, "sorting".length]);
  });

  it("selects the text when focus is already in the field, and keeps the query", async () => {
    await open({ section: "general" });
    await search("sorting");
    field().focus();
    field().setSelectionRange(3, 3);
    press(field());
    expect(document.activeElement).toBe(field());
    expect(field().value).toBe("sorting");
    expect(selection(field())).toEqual([0, "sorting".length]);
  });

  it("finds the field empty and leaves it empty when there is nothing to select", async () => {
    await open({ section: "general" });
    press(selectedTab()!);
    expect(document.activeElement).toBe(field());
    expect(field().value).toBe("");
  });

  it("goes by the physical key, whatever character it types", async () => {
    await open({ section: "general" });
    const event = press(selectedTab()!, { ...FIND, key: "\u0430" });
    expect(document.activeElement).toBe(field());
    expect(event.defaultPrevented).toBe(true);
  });

  it("leaves other keys alone: the letter on another key, an extra modifier or none", async () => {
    await open({ section: "general" });
    const tab = selectedTab()!;
    for (const init of [{ ...FIND, code: "KeyJ" }, { ...FIND, shiftKey: true }, { ...FIND, altKey: true }, { key: "f", code: "KeyF" }]) {
      const event = press(tab, init);
      expect(document.activeElement).toBe(tab);
      expect(event.defaultPrevented).toBe(false);
    }
  });

  it("ignores a key an input method is composing with", async () => {
    await open({ section: "general" });
    const tab = selectedTab()!;
    for (const init of [{ ...FIND, isComposing: true }, { ...FIND, keyCode: 229 }]) {
      const event = press(tab, init);
      expect(document.activeElement).toBe(tab);
      expect(event.defaultPrevented).toBe(false);
    }
  });

  it("does nothing extra while a modal is open over Settings, where focus stays", async () => {
    framesAsTimers();
    await open({ section: "trusted-folders", state: { trustedDirectories: ["/Users/someone/zzfolder"], homeDir: "/Users/someone" } });
    await click(panel()!.querySelector('button[aria-label^="Remove trusted folder"]'));
    const modal = document.querySelectorAll<HTMLElement>('[role="alertdialog"], [role="dialog"]');
    const inFront = modal[modal.length - 1]!;
    expect(inFront.textContent).toContain("Remove trusted folder");
    const control = inFront.querySelector<HTMLElement>("button")!;
    control.focus();
    await settleFrames();
    expect(document.activeElement).toBe(control);
    const event = press(control);
    await settleFrames();
    expect(document.activeElement).toBe(control);
    expect(document.activeElement).not.toBe(field());
    expect(event.defaultPrevented).toBe(false);
  });
});

describe("focus a re-match takes away", () => {
  it("goes to the search field, with the keyboard ring, when a new language no longer matches the row that held it", async () => {
    framesAsTimers();
    const previous = currentLanguage();
    onTestFinished(() => { act(() => setLanguage(previous)); });
    await open({ section: "general" });
    await search("language");
    const row = panel()!.querySelector<HTMLElement>('[data-settings-search-id="language"]')!;
    const trigger = row.querySelector<HTMLElement>("button")!;
    act(() => trigger.focus());
    act(() => setLanguage("de"));
    await settleFrames();
    expect(shownEntries()).not.toContain("language");
    // The field is found by its type, its name now being German.
    const german = document.querySelector<HTMLInputElement>('input[type="search"]')!;
    expect(document.activeElement).toBe(german);
    expect(german.getAttribute("data-focus-visible")).toBe("true");
  });

  /** A tap, so the move goes where no on-screen keyboard rises. */
  function tap() {
    document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "touch" }));
    onTestFinished(() => { document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse" })); });
  }

  async function focusLanguageRowAfterTap(query: string) {
    framesAsTimers();
    const previous = currentLanguage();
    onTestFinished(() => { act(() => setLanguage(previous)); });
    await open({ section: "general" });
    await search(query);
    const row = panel()!.querySelector<HTMLElement>('[data-settings-search-id="language"]')!;
    act(() => row.querySelector<HTMLElement>("button")!.focus());
    tap();
  }

  it("goes to Close after a tap when the new language leaves nothing found, the tabs being hidden", async () => {
    await focusLanguageRowAfterTap("language");
    act(() => setLanguage("de"));
    await settleFrames();
    expect(shownTabs()).toEqual([]);
    expect(document.activeElement).toBe(document.querySelector("[data-settings-close]"));
  });

  it("goes to the newly selected tab after a tap when the new language moves the shown section", async () => {
    await focusLanguageRowAfterTap("lan");
    act(() => setLanguage("de"));
    await settleFrames();
    expect(selectedTab()).not.toBeNull();
    expect(shownTabs()).toEqual([selectedTab()]);
    expect(selectedTab()?.textContent).not.toContain("General");
    expect(document.activeElement).toBe(selectedTab());
  });
});

describe("a row that waits on a read of the system", () => {
  it("is not counted by the search until the read it waits on has answered, and then counts and appears together (launch at login)", async () => {
    const read = deferredRead<"disabled">();
    const platform: PlatformAdapter = {
      kind: "tauri",
      host: MAC_HOST,
      launchAtLogin: { status: () => read.promise, set: async () => "disabled", openSystemSettings: async () => {} },
    };
    fakeTimers();
    await open({ section: "app-behavior", platform });
    await act(async () => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
    await search("launch at login");
    // No tab and no count for a row that is not on screen: the search finds nothing yet.
    expect(shownTabs()).toEqual([]);
    expect(status()).toBe("No setting matches the search.");
    expect(shownEntries()).toEqual([]);
    await read.resolve("disabled");
    expect(tabNames()).toEqual(["App behavior, 1 matching setting"]);
    expect(shownEntries()).toEqual(["launch-at-login"]);
    expect(panel()!.querySelector('[data-settings-search-id="launch-at-login"]')).not.toBeNull();
  });

  it("is not counted until the macOS permissions have been read, and then counts and appears together", async () => {
    const read = deferredRead<typeof GRANTED>();
    const platform: PlatformAdapter = {
      kind: "tauri",
      host: MAC_HOST,
      macosPermissions: { status: () => read.promise, request: async () => false, openSystemSettings: async () => {} },
    };
    fakeTimers();
    await open({ section: "general", platform });
    await act(async () => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
    await search("screen recording");
    expect(shownTabs()).toEqual([]);
    expect(status()).toBe("No setting matches the search.");
    await read.resolve({ screenRecording: false, accessibility: true });
    expect(tabNames()).toEqual(["Permissions, 1 matching setting"]);
    expect(selectedTab()?.textContent).toContain("Permissions");
    expect(shownEntries()).toEqual(["permissions.screenRecording"]);
  });

  it("reads once when Settings opens on Permissions, the section and the search sharing the dialog's read", async () => {
    const read = vi.fn(async () => ({ screenRecording: true, accessibility: true }));
    const platform: PlatformAdapter = { kind: "tauri", host: MAC_HOST, macosPermissions: { status: read, request: async () => false, openSystemSettings: async () => {} } };
    await open({ section: "permissions", platform });
    expect(read).toHaveBeenCalledTimes(1);
  });
});

/** A desktop shell on macOS whose permissions read is `status`, the other calls of the capability doing nothing. */
function macPlatform(status: MacosPermissionsCapability["status"]): PlatformAdapter {
  return { kind: "tauri", host: MAC_HOST, macosPermissions: { status, request: async () => false, openSystemSettings: async () => {} } };
}
const GRANTED = { screenRecording: true, accessibility: true };
const unreadable = () => Promise.reject(new Error("no such command"));
const refocus = () => act(async () => void window.dispatchEvent(new Event("focus")));
const settle = async () => { for (let turn = 0; turn < 3; turn += 1) await act(flushPromises); };
const alertText = () => panel()?.querySelector('[data-slot="alert-root"]')?.textContent;
const tabLabelled = (name: string) => tabs().find((tab) => tab.textContent?.includes(name))!;

/** A read that answers when `resolve` is called, which returns once the dialog has taken the answer. */
function deferredRead<T>() {
  let answer!: (value: T) => void;
  const promise = new Promise<T>((done) => { answer = done; });
  return { promise, resolve: async (value: T) => { answer(value); await settle(); } };
}

describe("a launch-at-login read that fails", () => {
  it("still shows the row, off, and counts it", async () => {
    const platform: PlatformAdapter = {
      kind: "tauri",
      host: MAC_HOST,
      launchAtLogin: { status: () => Promise.reject(new Error("no such command")), set: async () => "disabled", openSystemSettings: async () => {} },
    };
    await open({ section: "app-behavior", platform });
    await search("launch at login");
    expect(tabNames()).toEqual(["App behavior, 1 matching setting"]);
    expect(shownEntries()).toEqual(["launch-at-login"]);
    const toggle = panel()!.querySelector<HTMLInputElement>('[data-settings-search-id="launch-at-login"] input[role="switch"]');
    expect(toggle).not.toBeNull();
    expect(toggle!.checked).toBe(false);
  });
});

describe("when the permissions cannot be read", () => {
  it.each(["accessibility", "screen", "permissions"])("a search for %s lists the Permissions tab with 1 match and shows the alert, never an empty state", async (query) => {
    fakeTimers();
    await open({ section: "permissions", platform: macPlatform(unreadable) });
    await act(async () => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
    await search(query);
    expect(tabNames()).toContain("Permissions, 1 matching setting");
    expect(selectedTab()?.textContent).toContain("Permissions");
    expect(shownEntries()).toEqual(["permissions.unreadable"]);
    expect(alertText()).toContain("Could not read the permissions");
    expect(document.body.textContent).not.toContain("No matching settings");
    expect(status()).not.toBe("No setting matches the search.");
  });

  it("brings the Permissions section on screen when the selected one has no match, and announces the one match", async () => {
    fakeTimers();
    await open({ section: "general", platform: macPlatform(unreadable) });
    await act(async () => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
    await search("accessibility");
    expect(tabNames()).toEqual(["Permissions, 1 matching setting"]);
    expect(selectedTab()?.textContent).toContain("Permissions");
    expect(alertText()).toContain("Could not read the permissions");
    expect(status()).toBe("1 matching setting");
  });

  it("hides the alert, which stays in the section, from a query that matches none of it", async () => {
    await open({ section: "permissions", platform: macPlatform(unreadable) });
    const block = () => document.querySelector<HTMLElement>('[data-settings-search-id="permissions.unreadable"]');
    expect(block()).not.toBeNull();
    expect(block()!.hidden).toBe(false);
    await search("zzzznope");
    expect(block()).not.toBeNull();
    expect(block()!.hidden).toBe(true);
    expect(shownTabs()).toEqual([]);
    await search("accessibility");
    expect(block()!.hidden).toBe(false);
  });

  it.each([
    ["is still pending", () => new Promise<never>(() => {})],
    ["has reported the grants", async () => GRANTED],
  ])("does not count the alert while the read %s", async (_name, read) => {
    await open({ section: "permissions", platform: macPlatform(read) });
    await search("could not read the permissions");
    expect(shownTabs()).toEqual([]);
    expect(shownEntries()).toEqual([]);
  });
});

describe("a read that later takes the permission rows away", () => {
  it("moves the count from the rows to the alert, and the rows out of the panel", async () => {
    const read = vi.fn<MacosPermissionsCapability["status"]>(async () => GRANTED);
    await open({ section: "permissions", platform: macPlatform(read) });
    await search("lets agents");
    expect(tabNames()).toEqual(["Permissions, 2 matching settings"]);
    expect(shownEntries()).toEqual(["permissions.screenRecording", "permissions.accessibility"]);
    read.mockImplementation(unreadable);
    await refocus();
    await settle();
    expect(tabNames()).toEqual(["Permissions, 1 matching setting"]);
    expect(shownEntries()).toEqual(["permissions.unreadable"]);
    expect(alertText()).toContain("Could not read the permissions");
  });
});

describe("when the permissions are read again", () => {
  const mountWithRead = async (section: SettingsSectionId) => {
    const read = vi.fn<MacosPermissionsCapability["status"]>(async () => GRANTED);
    await open({ section, platform: macPlatform(read) });
    return read;
  };

  it("reads once when Settings opens, and not on a window focus while another section is shown and nothing is searched", async () => {
    const read = await mountWithRead("general");
    expect(read).toHaveBeenCalledTimes(1);
    await refocus();
    await settle();
    expect(read).toHaveBeenCalledTimes(1);
  });

  it("reads again on a window focus while the Permissions section is shown", async () => {
    const read = await mountWithRead("permissions");
    await refocus();
    await settle();
    expect(read).toHaveBeenCalledTimes(2);
  });

  it("does not read again on switching to the Permissions tab when no focus came back", async () => {
    const read = await mountWithRead("general");
    await click(tabLabelled("Permissions"));
    await settle();
    expect(selectedTab()?.textContent).toContain("Permissions");
    expect(read).toHaveBeenCalledTimes(1);
  });

  it("reads once on switching to the Permissions tab after a focus return while nobody followed", async () => {
    const read = await mountWithRead("general");
    await refocus();
    await settle();
    expect(read).toHaveBeenCalledTimes(1);
    await click(tabLabelled("Permissions"));
    await settle();
    expect(read).toHaveBeenCalledTimes(2);
    await click(tabLabelled("General"));
    await click(tabLabelled("Permissions"));
    await settle();
    expect(read).toHaveBeenCalledTimes(2);
  });

  it("reads once on starting a search after a focus return while nobody followed", async () => {
    const read = await mountWithRead("general");
    await refocus();
    await settle();
    expect(read).toHaveBeenCalledTimes(1);
    await search("sorting");
    await settle();
    expect(read).toHaveBeenCalledTimes(2);
    await search("");
    await search("sorting");
    await settle();
    expect(read).toHaveBeenCalledTimes(2);
  });

  it("reads again on a window focus while the search has words, and stops once it is cleared", async () => {
    const read = await mountWithRead("general");
    await search("sorting");
    await refocus();
    await settle();
    expect(read).toHaveBeenCalledTimes(2);
    await click(clearButton());
    await refocus();
    await settle();
    expect(read).toHaveBeenCalledTimes(2);
  });

  it("keeps reading for the section when the search ends, the two not cancelling each other", async () => {
    const read = await mountWithRead("permissions");
    await search("lets agents");
    await refocus();
    await settle();
    expect(read).toHaveBeenCalledTimes(2);
    await click(clearButton());
    await refocus();
    await settle();
    expect(read).toHaveBeenCalledTimes(3);
  });

  it("keeps reading for the search when the section leaves, until the search ends", async () => {
    const read = await mountWithRead("permissions");
    await search("e");
    await click(tabLabelled("General"));
    await settle();
    expect(selectedTab()?.textContent).toContain("General");
    expect(panel()!.querySelector('[data-settings-search-id="permissions.accessibility"]')).toBeNull();
    await refocus();
    await settle();
    expect(read).toHaveBeenCalledTimes(2);
    await click(clearButton());
    await refocus();
    await settle();
    expect(read).toHaveBeenCalledTimes(2);
  });
});

describe("the launch-at-login read", () => {
  it("is made once for the dialog when it opens on App behavior, and not again by moving between sections", async () => {
    const status = vi.fn<LaunchAtLoginCapability["status"]>(async () => "disabled");
    const platform: PlatformAdapter = { kind: "tauri", host: MAC_HOST, launchAtLogin: { status, set: async () => "disabled", openSystemSettings: async () => {} } };
    await open({ section: "app-behavior", platform });
    expect(status).toHaveBeenCalledTimes(1);
    expect(shownEntries()).toContain("launch-at-login");
    await click(tabLabelled("General"));
    await click(tabLabelled("App behavior"));
    expect(shownEntries()).toContain("launch-at-login");
    expect(status).toHaveBeenCalledTimes(1);
  });
});

describe("the section on screen while a read answers", () => {
  it("stays where it is when the Permissions section appears in the list ahead of it", async () => {
    const read = deferredRead<typeof GRANTED>();
    await open({ section: "about", platform: macPlatform(() => read.promise) });
    await search("check");
    expect(tabNames()).toEqual(["Cleanup, 1 matching setting"]);
    expect(selectedTab()?.textContent).toContain("Cleanup");
    await read.resolve(GRANTED);
    expect(tabNames()).toEqual(["Permissions, 1 matching setting", "Cleanup, 1 matching setting"]);
    expect(selectedTab()?.textContent).toContain("Cleanup");
    expect(shownEntries()).toEqual(["auto_delete_unresumable_archives"]);
  });

  it("stays where it is when the read answers, though the section the dialog opened on is now listed", async () => {
    const read = deferredRead<typeof GRANTED>();
    await open({ section: "permissions", platform: macPlatform(() => read.promise) });
    await search("check");
    expect(tabNames()).toEqual(["Cleanup, 1 matching setting"]);
    expect(selectedTab()?.textContent).toContain("Cleanup");
    await read.resolve(GRANTED);
    expect(tabNames()).toEqual(["Permissions, 1 matching setting", "Cleanup, 1 matching setting"]);
    expect(selectedTab()?.textContent).toContain("Cleanup");
  });

  it("shows the section whose tab is pressed, though it is the one the dialog opened on before a search moved away from it", async () => {
    const read = deferredRead<typeof GRANTED>();
    await open({ section: "permissions", platform: macPlatform(() => read.promise) });
    await search("check");
    await read.resolve(GRANTED);
    expect(selectedTab()?.textContent).toContain("Cleanup");
    await click(tabLabelled("Permissions"));
    expect(selectedTab()?.textContent).toContain("Permissions");
    expect(panel()?.textContent).not.toContain("Cleanup");
    expect(shownEntries()).toEqual(["permissions.screenRecording"]);
  });

  it("falls back to the first listed section only when the one shown is no longer listed", async () => {
    await open({ section: "about", platform: macPlatform(async () => GRANTED) });
    await search("sorting");
    expect(selectedTab()?.textContent).toContain("General");
    await search("check");
    expect(tabNames()).toEqual(["Permissions, 1 matching setting", "Cleanup, 1 matching setting"]);
    expect(selectedTab()?.textContent).toContain("Permissions");
  });
});
