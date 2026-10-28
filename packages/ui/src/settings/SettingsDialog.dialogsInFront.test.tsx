// @vitest-environment jsdom
import { act, StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { framesAsTimers, settleFrames } from "../dialogs/dialogTestSupport";
import { account, accountLogins, bus, settings } from "./agentAccountsSectionTestState";
import { SettingsDialog, type SettingsSectionId } from "./SettingsDialog";

vi.mock("../store", async (importOriginal) => (await import("./agentAccountsSectionTestState")).storeStandIn(await importOriginal()));
vi.mock("../components/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../components/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../components/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../dialogs/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

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
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  onTestFinished(() => { document.documentElement.style.removeProperty("--breakpoint-docked"); });
});

/** Opens Settings on the Agent accounts section as a press on a control of another dialog does: that control goes in
 * the same update, so focus has nothing to stay on as Settings and the dialog in front of it mount. */
function openSettings(navigation: { account?: string; addAgent?: boolean }, strict: boolean) {
  const opener = document.body.appendChild(document.createElement("button"));
  opener.focus();
  // Stubbed after `framesAsTimers`, which puts every global back when the test finishes.
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: media.startsWith("(min-width:"), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const section: SettingsSectionId = "accounts";
  const dialog = <SettingsDialog navigation={{ nonce: 1, section, addAgent: navigation.addAgent ?? false, account: navigation.account }} onClose={() => {}} />;
  act(() => {
    opener.remove();
    root.render(strict ? <StrictMode>{dialog}</StrictMode> : dialog);
    bus.notify();
  });
}

/** The dialog in front of Settings: the last one, as it is portalled after Settings'. */
const inFront = () => [...document.querySelectorAll('[role="dialog"]')].at(-1)!;
const dialogCount = () => document.querySelectorAll('[role="dialog"]').length;
const sectionTab = () => [...document.querySelectorAll<HTMLElement>('[role="tab"][aria-selected="true"]')].find((tab) => tab.textContent === "Agent accounts");
const escape = () => act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, key: "Escape" })));
const pressButton = (name: string) => act(async () => [...inFront().querySelectorAll<HTMLElement>("button")].find((button) => button.textContent === name || button.getAttribute("aria-label") === name)!.click());

const presses = [["a press without a pointer", "virtual"], ["a key press", "keyboard"], ["a pointer press", "pointer"]] as const;
/** Whether the control focus was handed to shows its ring: the hand-off is not one the user aimed. */
const ringShown = () => getInteractionModality() === "keyboard";

describe("closing the account settings dialog View in Settings opened", () => {
  const sharingSwitch = () => document.querySelector<HTMLElement>('[data-sharing-account="codex-work"] input[role="switch"]');
  for (const [press, modality] of presses) {
    for (const strict of [false, true]) {
      const after = `after ${press}${strict ? ", under StrictMode" : ""}`;
      for (const [how, close] of [["Escape", escape], ["its close button", () => pressButton("Close")]] as const) {
        it(`by ${how} gives focus to the section's tab ${after}`, async () => {
          framesAsTimers(modality);
          openSettings({ account: "codex-work" }, strict);
          await settleFrames();
          expect(document.activeElement).toBe(sharingSwitch());
          await close();
          await settleFrames();
          expect(dialogCount()).toBe(1);
          expect(document.activeElement).toBe(sectionTab());
          expect(ringShown()).toBe(true);
        });
      }
    }
  }

  it("gives focus to the section's tab when it closes by itself, its account removed by another window", async () => {
    framesAsTimers("virtual");
    openSettings({ account: "codex-work" }, false);
    await settleFrames();
    settings.accounts = settings.accounts.filter((held) => held.id !== "codex-work");
    notifyLater();
    await settleFrames();
    expect(dialogCount()).toBe(1);
    expect(document.activeElement).toBe(sectionTab());
  });
});

/** Tells the section the daemon's state changed the way a push does: outside any press, so React renders what follows
 * in a task of its own, which a frame may come before. */
function notifyLater() {
  const globals = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
  globals.IS_REACT_ACT_ENVIRONMENT = false;
  try {
    bus.notify();
  } finally {
    globals.IS_REACT_ACT_ENVIRONMENT = true;
  }
}

describe("closing the login dialog of a login found running as Settings opens", () => {
  const running = { agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null } as const;
  for (const [press, modality] of presses) {
    for (const strict of [false, true]) {
      it(`by Cancel login gives focus to the section's tab after ${press}${strict ? ", under StrictMode" : ""}`, async () => {
        framesAsTimers(modality);
        accountLogins.push({ ...running });
        openSettings({}, strict);
        await settleFrames();
        expect(dialogCount()).toBe(2);
        await pressButton("Cancel login");
        accountLogins.length = 0;
        act(() => bus.notify());
        await settleFrames();
        expect(dialogCount()).toBe(1);
        expect(document.activeElement).toBe(sectionTab());
        expect(ringShown()).toBe(true);
      });
    }
  }

  for (const outcome of ["succeeded", "cancelled"] as const) {
    it(`gives focus to the section's tab when it closes by itself, the login ${outcome} elsewhere`, async () => {
      framesAsTimers("virtual");
      accountLogins.push({ ...running });
      openSettings({}, false);
      await settleFrames();
      expect(dialogCount()).toBe(2);
      accountLogins.splice(0, 1, { ...running, status: outcome });
      notifyLater();
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
      it(`gives focus to the empty panel's Add agent button, with its ring, after ${press}${strict ? ", under StrictMode" : ""}`, async () => {
        settings.added_agents = [];
        framesAsTimers(modality);
        openSettings({ addAgent: true }, strict);
        await settleFrames();
        expect(inFront().textContent).toContain("Only an agent installed on this machine can be added.");
        await pressButton("Close");
        await settleFrames();
        expect(dialogCount()).toBe(1);
        expect(emptyPanelButton()).toBeDefined();
        expect(document.activeElement).toBe(emptyPanelButton());
        expect(ringShown()).toBe(true);
      });
    }
  }
});
