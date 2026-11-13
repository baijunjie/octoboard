// @vitest-environment jsdom
import { act, StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { framesAsTimers, settleFrames } from "../components/dialog/dialogTestSupport";
import type { Daemon, State } from "../store";
import { TestHost } from "../test/TestHost";
import { account, accountLogins, settings, startDaemon } from "./accounts/agentAccountsSectionTestState";
import { SettingsDialog } from "./SettingsDialog";
import type { SettingsSectionId } from "./sections";
import { matchesWidthQuery } from "../test/viewport";

vi.mock("../components/icons/AgentIcon", async (importOriginal) => (await import("./accounts/agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./accounts/agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./accounts/agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./accounts/agentAccountsSectionTestState")).OptionSelectStandIn }));

// The dialogs the Agent accounts section opens in front of Settings by itself as Settings opens — an account's settings
// dialog for View in Settings, the Add agent dialog, the login dialog of a login found running — mount a commit after
// Settings, before Settings has focused anything of its own. Closing one must leave focus inside Settings, where
// Escape still reaches it, however Settings was opened.

beforeEach(() => {
  const globals = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
  const actEnvironment = globals.IS_REACT_ACT_ENVIRONMENT;
  globals.IS_REACT_ACT_ENVIRONMENT = true;
  onTestFinished(() => {
    if (actEnvironment === undefined) delete globals.IS_REACT_ACT_ENVIRONMENT;
    else globals.IS_REACT_ACT_ENVIRONMENT = actEnvironment;
  });
  settings.added_agents = ["claude", "codex", "grok"];
  settings.accounts = [account("claude-work", "claude"), account("codex-work", "codex")];
  accountLogins.length = 0;
  document.documentElement.style.setProperty("--breakpoint-tablet", "768px");
  document.documentElement.style.setProperty("--breakpoint-phone", "390px");
  onTestFinished(() => {
    document.documentElement.style.removeProperty("--breakpoint-tablet");
    document.documentElement.style.removeProperty("--breakpoint-phone");
  });
});

/** The daemon the dialog under test was opened over. */
let daemon: Daemon;

/** Opens Settings on the Agent accounts section as a press on a control of another dialog does: that control goes in
 * the same update, so focus has nothing to stay on as Settings and the dialog in front of it mount. */
function openSettings(navigation: { account?: string; addAgent?: boolean }, strict: boolean) {
  const opener = document.body.appendChild(document.createElement("button"));
  opener.focus();
  // Stubbed after `framesAsTimers`, which puts every global back when the test finishes.
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: matchesWidthQuery(media, 1440), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const section: SettingsSectionId = "accounts";
  daemon = startDaemon();
  const dialog = <TestHost daemon={daemon}><SettingsDialog navigation={{ nonce: 1, section, addAgent: navigation.addAgent ?? false, account: navigation.account }} onClose={() => {}} /></TestHost>;
  act(() => {
    opener.remove();
    root.render(strict ? <StrictMode>{dialog}</StrictMode> : dialog);
  });
}

/** The dialog in front of Settings: the last one, as it is portalled after Settings'. */
const inFront = () => [...document.querySelectorAll('[role="dialog"]')].at(-1)!;
const dialogCount = () => document.querySelectorAll('[role="dialog"]').length;
const sectionTab = () => [...document.querySelectorAll<HTMLElement>('[role="tab"][aria-selected="true"]')].find((tab) => tab.textContent === "Agent accounts");
const escape = () => act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, key: "Escape" })));
const pressButton = (name: string) => act(async () => [...inFront().querySelectorAll<HTMLElement>("button")].find((button) => button.textContent === name || button.getAttribute("aria-label") === name)!.click());

const presses = [["a press without a pointer", "virtual"], ["a key press", "keyboard"], ["a pointer press", "pointer"]] as const;
/** Whether the control focus was handed to shows its ring. The handoff is not one the user aimed, so the ring follows
 * the last input (`ringForAutomaticFocus`): shown after a hardware key, never after a mouse press. */
const ringShown = () => getInteractionModality() === "keyboard";
/** Sets the press that opens Settings, as `framesAsTimers` does. A press without a pointer (a screen reader's) leaves
 * the last input as it was, so a hardware key is sent before it, as a screen reader's user navigates with keys: the
 * ring then does not depend on what an earlier test left behind. */
function pressFrom(modality: (typeof presses)[number][1]) {
  if (modality === "virtual") document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
  framesAsTimers(modality);
}
/** Whether the handoff after `modality`'s press shows the ring: only a mouse press turns it off. */
const ringAfter = (modality: (typeof presses)[number][1]) => modality !== "pointer";

describe("closing the account settings dialog View in Settings opened", () => {
  const sharingSwitch = () => document.querySelector<HTMLElement>('[data-sharing-account="codex-work"] input[role="switch"]');
  for (const [press, modality] of presses) {
    for (const strict of [false, true]) {
      const after = `after ${press}${strict ? ", under StrictMode" : ""}`;
      for (const [how, close] of [["Escape", escape], ["its close button", () => pressButton("Close")]] as const) {
        it(`by ${how} gives focus to the section's tab ${after}`, async () => {
          pressFrom(modality);
          openSettings({ account: "codex-work" }, strict);
          await settleFrames();
          expect(document.activeElement).toBe(sharingSwitch());
          await close();
          await settleFrames();
          expect(dialogCount()).toBe(1);
          expect(document.activeElement).toBe(sectionTab());
          // Escape is a key, the last input then; the close button's `click()` has no pointer behind it.
          expect(ringShown()).toBe(how === "Escape" || ringAfter(modality));
        });
      }
    }
  }

  it("gives focus to the section's tab when it closes by itself, its account removed by another window", async () => {
    pressFrom("virtual");
    openSettings({ account: "codex-work" }, false);
    await settleFrames();
    updateLater({ settings: { ...daemon.store.getState().settings, accounts: daemon.store.getState().settings.accounts.filter((held) => held.id !== "codex-work") } });
    await settleFrames();
    expect(dialogCount()).toBe(1);
    expect(document.activeElement).toBe(sectionTab());
  });
});

/** Puts `patch` to the store the way a push does: outside any press, so React renders what follows in a task of its
 * own, which a frame may come before. */
function updateLater(patch: Partial<State>) {
  const globals = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
  globals.IS_REACT_ACT_ENVIRONMENT = false;
  try {
    daemon.store.setState(patch);
  } finally {
    globals.IS_REACT_ACT_ENVIRONMENT = true;
  }
}

describe("closing the login dialog of a login found running as Settings opens", () => {
  const running = { agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null } as const;
  for (const [press, modality] of presses) {
    for (const strict of [false, true]) {
      it(`by Cancel login gives focus to the section's tab after ${press}${strict ? ", under StrictMode" : ""}`, async () => {
        pressFrom(modality);
        accountLogins.push({ ...running });
        openSettings({}, strict);
        await settleFrames();
        expect(dialogCount()).toBe(2);
        await pressButton("Cancel login");
        act(() => daemon.store.setState({ accountLogins: [] }));
        await settleFrames();
        expect(dialogCount()).toBe(1);
        expect(document.activeElement).toBe(sectionTab());
        expect(ringShown()).toBe(ringAfter(modality));
      });
    }
  }

  for (const outcome of ["succeeded", "cancelled"] as const) {
    it(`gives focus to the section's tab when it closes by itself, the login ${outcome} elsewhere after a key`, async () => {
      pressFrom("virtual");
      accountLogins.push({ ...running });
      openSettings({}, false);
      await settleFrames();
      expect(dialogCount()).toBe(2);
      updateLater({ accountLogins: [{ ...running, status: outcome }] });
      await settleFrames();
      expect(dialogCount()).toBe(1);
      expect(document.activeElement).toBe(sectionTab());
      expect(ringShown()).toBe(true);
    });
  }
});

describe("closing the Add agent dialog put in front of Settings with no agent added", () => {
  const emptyPanelButton = () => [...document.querySelectorAll<HTMLElement>('[role="tabpanel"] button')].find((button) => button.textContent === "Add agent");
  for (const [press, modality] of presses) {
    for (const strict of [false, true]) {
      it(`gives focus to the empty panel's Add agent button, its ring following the last input, after ${press}${strict ? ", under StrictMode" : ""}`, async () => {
        settings.added_agents = [];
        pressFrom(modality);
        openSettings({ addAgent: true }, strict);
        await settleFrames();
        expect(inFront().textContent).toContain("Only an installed agent can be added.");
        await pressButton("Close");
        await settleFrames();
        expect(dialogCount()).toBe(1);
        expect(emptyPanelButton()).toBeDefined();
        expect(document.activeElement).toBe(emptyPanelButton());
        expect(ringShown()).toBe(ringAfter(modality));
      });
    }
  }
});
