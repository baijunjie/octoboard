// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import type { AccountLogin } from "../protocol";
import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { ErrorAlerts } from "../dialogs/ErrorAlerts";
import { trackMountRefocus } from "../dialogs/dialogTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { windowAttention } from "../components/windowTestSupport";
import { SettingsDialog } from "./SettingsDialog";
import { registerSettingsLauncher } from "./settingsLauncher";
import { accountLogins, bus, dismissAccountLogin, request, settings, toastError, trackAccountLogin } from "./agentAccountsSectionTestState";
import { buttonNamed, chooseItem, mount, openTabMenu, rowOf } from "./agentAccountsSectionTestSupport";

vi.mock("../store", async (importOriginal) => (await import("./agentAccountsSectionTestState")).storeStandIn(await importOriginal()));
vi.mock("../components/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../components/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../components/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("./useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../dialogs/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

/** Fakes the timers for one test, checking once its root is unmounted that it left none running. Call it before
 * `mount`, so that its check runs after the unmount. */
function fakeTimers() {
  vi.useFakeTimers();
  onTestFinished(() => {
    try {
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });
}

describe("an account's actions menu", () => {
  // The account's name is isolated in the label, and the tabs' own hidden menus are named alike.
  const triggers = (container: HTMLElement) => [...container.querySelectorAll<HTMLElement>('button[aria-label^="Actions for"]:not([hidden])')].filter((button) => button.getAttribute("aria-label")!.includes("("));
  async function openAccountMenu(container: HTMLElement, at = 1) {
    const trigger = triggers(container)[at]!;
    await act(async () => {
      trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
      trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
      trigger.click();
    });
  }

  const items = () => [...document.querySelectorAll("[role=menuitem]")].map((entry) => entry.textContent);

  it("holds Settings, Edit, Log in and Remove for an owned account, in place of buttons of their own", async () => {
    const { container } = mount();
    expect(buttonNamed(container, "Edit")).toBeUndefined();
    await openAccountMenu(container, 1);
    expect(items()).toEqual(["Settings", "Edit", "Log in", "Remove"]);
  });

  it("holds Settings, Rename and Log in for the default account, which has no directory or removal to offer", async () => {
    const { container } = mount();
    await openAccountMenu(container, 0);
    expect(items()).toEqual(["Settings", "Rename", "Log in"]);
  });

  it("is offered by the default account and each owned account", () => {
    const { container } = mount();
    expect(triggers(container)).toHaveLength(2);
    expect(triggers(container)[0]!.getAttribute("aria-label")).toContain("Default");
    expect(triggers(container)[1]!.getAttribute("aria-label")).toContain("claude-work");
  });

  it("starts the login of the account it was chosen on in a modal, the default account's with no account", async () => {
    const { container } = mount();
    await openAccountMenu(container, 1);
    await chooseItem("Log in");
    expect(request).toHaveBeenLastCalledWith({ type: "start_account_login", agent: "claude", account: "claude-work" });
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain("Log in to Claude Code account claude-work");
    expect(dialog.textContent).toContain("Logging in…");
    await act(async () => buttonNamed(dialog, "Cancel login")!.click());
    expect(request).toHaveBeenLastCalledWith({ type: "cancel_account_login", agent: "claude", account: "claude-work" });
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    await openAccountMenu(container, 0);
    await chooseItem("Log in");
    expect(request).toHaveBeenLastCalledWith({ type: "start_account_login", agent: "claude", account: null });
  });

  it("shows the default account under the name the user gave it, and renames it from its menu", async () => {
    settings.default_account_names = { claude: "Personal" };
    const { container } = mount();
    expect(container.textContent).toContain("Personal");
    expect(triggers(container)[0]!.getAttribute("aria-label")).toContain("Personal");
    await openAccountMenu(container, 0);
    await chooseItem("Rename");
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain("Rename the default Claude Code account");
    // Only the name is edited: the default account's directory is not.
    expect(dialog.querySelectorAll("input")).toHaveLength(1);
    expect((dialog.querySelector("input") as HTMLInputElement).value).toBe("Personal");
  });

  it("refuses, in place, a default name an account of the agent has", async () => {
    const { container } = mount();
    await openAccountMenu(container, 0);
    await chooseItem("Rename");
    const input = document.querySelector('[role="dialog"] input') as HTMLInputElement;
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      setter.call(input, "Claude-Work");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => buttonNamed(document.querySelector('[role="dialog"]')!, "Save")!.click());
    expect(request).not.toHaveBeenCalled();
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("already has an account named");
  });

  it("takes up a login found running when the section opens, with the page link, the device code and Cancel login", async () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: "https://claude.test/authorize", device_code: null, opened_by_agent: false, failure: null });
    const { container } = mount();
    const dialog = () => document.querySelector('[role="dialog"]')!;
    expect(request).not.toHaveBeenCalled();
    expect(dialog().textContent).toContain("Waiting for you to authorize in your browser");
    expect(dialog().querySelector('a[href="https://claude.test/authorize"]')?.textContent).toContain("Open the page again");
    expect([...dialog().querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent?.replace(/[\u2068\u2069]/g, ""))).toEqual(["Claude Code", "claude-work"]);
    await act(async () => buttonNamed(dialog(), "Cancel login")!.click());
    expect(request).toHaveBeenLastCalledWith({ type: "cancel_account_login", agent: "claude", account: "claude-work" });
    expect(container.querySelector('[role="dialog"]')).toBeNull();
  });

  it("shows the device code in the modal", () => {
    accountLogins.push({ agent: "claude", account: null, status: "waiting", authorization_url: null, device_code: "ABCD-1234", opened_by_agent: true, failure: null });
    mount();
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Confirm this code in your browser: ABCD-1234");
  });

  it("announces the login's progress from one region whose first text waits for the modal's own focus move", () => {
    vi.useFakeTimers();
    onTestFinished(() => void vi.useRealTimers());
    accountLogins.push({ agent: "claude", account: "claude-work", status: "starting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { render } = mount();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(document.activeElement).toBe(buttonNamed(dialog, "Cancel login"));
    // The spinner carries a role of its own, hidden.
    const region = dialog.querySelector<HTMLElement>('[role="status"]:not([aria-hidden])')!;
    const said = () => region.textContent?.replace(/[\u2068\u2069]/g, "");
    // Seen only: the visible line is not a second voice.
    expect(dialog.textContent).toContain("Logging in…");
    expect(region.previousElementSibling?.getAttribute("aria-hidden")).toBe("true");
    expect(said()).toBe("");
    act(() => vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS - 1));
    expect(said()).toBe("");
    act(() => vi.advanceTimersByTime(1));
    expect(said()).toBe("Logging in…");
    accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    render();
    expect(said()).toBe("Waiting for you to authorize in your browser…");
    accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: "ABCD-1234", opened_by_agent: false, failure: null });
    render();
    expect(said()).toBe("Confirm this code in your browser: ABCD-1234");
    expect([...dialog.querySelectorAll('[role="status"]:not([aria-hidden])')]).toEqual([region]);
  });

  it("closes the modal when the login succeeds", async () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { render } = mount();
    expect(document.querySelector('[role="dialog"]')).not.toBeNull();
    accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "succeeded", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    render();
    await act(async () => {});
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it("keeps a failed login in the modal with its reason worded from the catalog, and logs in again from it", async () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { render } = mount();
    accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "failed", authorization_url: null, device_code: null, opened_by_agent: false,
      failure: { code: "login_failed", params: { agent: "Claude Code", detail: "Login failed: 400" }, message: "x" } });
    render();
    await act(async () => {});
    const dialog = document.querySelector('[role="dialog"]')!;
    const text = dialog.textContent!.replace(/[\u2068\u2069]/g, "");
    expect(text).toContain("Could not log in to claude-work.");
    expect(text).toContain("Claude Code could not log in.");
    expect(text).toContain("Login failed: 400");
    expect(buttonNamed(dialog, "Cancel login")).toBeUndefined();
    await act(async () => buttonNamed(dialog, "Log in again")!.click());
    expect(dismissAccountLogin).toHaveBeenCalledOnce();
    expect(request).toHaveBeenLastCalledWith({ type: "start_account_login", agent: "claude", account: "claude-work" });
    await act(async () => buttonNamed(document.querySelector('[role="dialog"]')!, "Close")?.click());
  });

  it("leaves no line in the row for a login that ended without success", () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "failed", authorization_url: null, device_code: null, opened_by_agent: false,
      failure: { code: "login_failed", params: { agent: "Claude Code", detail: "Login failed: 400" }, message: "x" } });
    const { container } = mount();
    expect([...container.querySelectorAll('[role="status"]')].map((el) => el.textContent).join("")).toBe("");
    expect(buttonNamed(container, "Dismiss")).toBeUndefined();
    expect(container.textContent).not.toContain("Login failed: 400");
  });

  it("leaves no visible line for a login that succeeded, and keeps one already there when the row mounts unsaid", () => {
    fakeTimers();
    accountLogins.push({ agent: "claude", account: "claude-work", status: "succeeded", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { container } = mount();
    act(() => vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
    expect([...container.querySelectorAll('[role="status"]')].map((el) => el.textContent).join("")).toBe("");
    expect(buttonNamed(container, "Dismiss")).toBeUndefined();
    expect(buttonNamed(container, "Cancel login")).toBeUndefined();
  });

  it("announces a login that succeeded only once the modal has closed and given focus back, and clears it after", () => {
    fakeTimers();
    windowAttention();
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { container, render } = mount();
    const row = rowOf(container, "claude-work");
    const region = row.querySelector<HTMLElement>('[role="status"]')!;
    // The open modal hides the row from assistive technology.
    expect(region.closest('[aria-hidden="true"], [inert]')).not.toBeNull();
    accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "succeeded", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    render();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(region.closest('[aria-hidden="true"], [inert]')).toBeNull();
    expect(region.textContent).toBe("");
    act(() => vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
    expect(row.querySelector('[role="status"]')).toBe(region);
    expect(region.classList.contains("sr-only")).toBe(true);
    expect(region.textContent?.replace(/[\u2068\u2069]/g, "")).toBe("claude-work: logged in.");
    expect([...container.querySelectorAll('[role="status"]:not(:empty)')]).toEqual([region]);
    act(() => vi.advanceTimersByTime(6000));
    expect(dismissAccountLogin).toHaveBeenCalledOnce();
    render();
    expect(region.textContent).toBe("");
  });

  it("holds the announcement and the clearing of a login that succeeded while the window was not attended until the user is back", () => {
    fakeTimers();
    const attend = windowAttention();
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { container, render } = mount();
    const region = rowOf(container, "claude-work").querySelector('[role="status"]')!;
    attend(false);
    accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "succeeded", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    render();
    act(() => vi.advanceTimersByTime(60_000));
    expect(region.textContent).toBe("");
    expect(dismissAccountLogin).not.toHaveBeenCalled();
    attend(true);
    act(() => vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS - 1));
    expect(region.textContent).toBe("");
    act(() => vi.advanceTimersByTime(1));
    expect(region.textContent?.replace(/[\u2068\u2069]/g, "")).toBe("claude-work: logged in.");
    act(() => vi.advanceTimersByTime(6000));
    expect(dismissAccountLogin).toHaveBeenCalledOnce();
  });

  it("starts the login once under StrictMode, and shows how the login really ended, not a refusal of a second start", async () => {
    // The daemon refuses a second start of a login that is running.
    let starts = 0;
    request.mockImplementation(((message: { type: string }) => {
      if (message.type !== "start_account_login") return Promise.resolve({});
      starts += 1;
      return starts === 1 ? Promise.resolve({}) : Promise.reject(Object.assign(new Error("running"), { code: "login_already_running", params: { agent: "Claude Code" } }));
    }) as never);
    const { container, render } = mount(undefined, true);
    await openAccountMenu(container, 1);
    await chooseItem("Log in");
    await act(async () => {});
    expect(starts).toBe(1);
    accountLogins.push({ agent: "claude", account: "claude-work", status: "failed", authorization_url: null, device_code: null, opened_by_agent: false,
      failure: { code: "login_failed", params: { agent: "Claude Code", detail: "Login failed: 400" }, message: "x" } });
    render();
    await act(async () => {});
    const text = document.querySelector('[role="dialog"]')!.textContent!.replace(/[\u2068\u2069]/g, "");
    expect(text).toContain("Claude Code could not log in.");
    expect(text).toContain("Login failed: 400");
    expect(text).not.toContain("already running");
  });

  it("keeps the modal open for a login started on an account whose earlier login ended in a record the store still holds", async () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "cancelled", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { container } = mount();
    await openAccountMenu(container, 1);
    await chooseItem("Log in");
    await act(async () => {});
    expect(dismissAccountLogin).toHaveBeenCalledOnce();
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Logging in…");
  });

  it("cancels a login that was still being started when the modal was closed, once the daemon has it", async () => {
    let registered!: () => void;
    request.mockImplementation(((message: { type: string }) => message.type === "start_account_login"
      ? new Promise((resolve) => { registered = () => resolve({}); })
      : Promise.resolve({})) as never);
    const { container } = mount();
    await openAccountMenu(container, 1);
    await chooseItem("Log in");
    await act(async () => buttonNamed(document.querySelector('[role="dialog"]')!, "Cancel login")!.click());
    expect(trackAccountLogin).toHaveBeenLastCalledWith(expect.anything(), "claude", "claude-work", false);
    expect(request).not.toHaveBeenCalledWith({ type: "cancel_account_login", agent: "claude", account: "claude-work" });
    await act(async () => registered());
    expect(request).toHaveBeenLastCalledWith({ type: "cancel_account_login", agent: "claude", account: "claude-work" });
  });

  it("closes the modal when the login's record is gone after it was seen, and attaches to a login already running", async () => {
    request.mockRejectedValueOnce(Object.assign(new Error("running"), { code: "login_already_running", params: {} }));
    const { container, render } = mount();
    await openAccountMenu(container, 1);
    await chooseItem("Log in");
    await act(async () => {});
    expect(document.querySelector('[role="dialog"]')?.textContent).not.toContain("already running");
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    render();
    await act(async () => {});
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Waiting for you to authorize");
    accountLogins.length = 0;
    render();
    await act(async () => {});
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it("keeps a failed login in the modal when a snapshot drops its record", async () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { render } = mount();
    accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "failed", authorization_url: null, device_code: null, opened_by_agent: false,
      failure: { code: "login_failed", params: { agent: "Claude Code", detail: "Login failed: 400" }, message: "x" } });
    render();
    await act(async () => {});
    accountLogins.length = 0;
    render();
    await act(async () => {});
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain("Login failed: 400");
    expect(buttonNamed(dialog, "Log in again")).toBeDefined();
  });

  it("notes that this window started the login it asked for, and shows a refusal in the modal", async () => {
    const { container } = mount();
    request.mockRejectedValueOnce(Object.assign(new Error("busy"), { code: "login_codex_busy", params: {} }));
    await openAccountMenu(container, 0);
    await chooseItem("Log in");
    await act(async () => {});
    expect(trackAccountLogin).toHaveBeenLastCalledWith(expect.anything(), "claude", null, false);
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain("Another Codex account is logging in");
    expect(buttonNamed(dialog, "Log in again")).toBeDefined();
    expect(toastError).not.toHaveBeenCalled();
  });

  it("keeps the login status region out of display:none while it is empty, so its first text is announced", () => {
    const { container } = mount();
    const status = container.querySelector<HTMLElement>('[role="status"]')!;
    expect(status.textContent).toBe("");
    expect(status.closest(".hidden")).toBeNull();
    // Out of the grid's flow, so it takes no row of its own.
    expect(status.classList.contains("sr-only")).toBe(true);
  });

  it("opens the account form from Edit and the confirmation from Remove", async () => {
    const { container } = mount();
    await openAccountMenu(container);
    await chooseItem("Edit");
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Edit Claude Code account");
    act(() => buttonNamed(document, "Cancel")!.click());
    await openAccountMenu(container);
    await chooseItem("Remove");
    expect(document.querySelector('[role="alertdialog"], [role="dialog"]')?.textContent).toContain("Remove account");
    await act(async () => buttonNamed(document.querySelector('[role="alertdialog"], [role="dialog"]')!, "Remove")!.click());
    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "delete_account", account: "claude-work" });
  });

  it.each(["account_still_sharing", "account_sharing_failed"] as const)("reports %s when removing the account is refused", async (code) => {
    const alerts = document.body.appendChild(document.createElement("div"));
    const alertsRoot = createRoot(alerts);
    onTestFinished(() => act(() => alertsRoot.unmount()));
    act(() => alertsRoot.render(<ErrorAlerts />));
    const { container } = mount();
    request.mockRejectedValueOnce(new DaemonRequestError("busy", code, { account: "claude-work", name: "claude-work" }));
    await openAccountMenu(container);
    await chooseItem("Remove");
    await act(async () => buttonNamed(document.querySelector('[role="alertdialog"], [role="dialog"]')!, "Remove")!.click());
    const shown = [...document.querySelectorAll('[role="alertdialog"]')].map((alert) => alert.textContent ?? "").join("\n");
    expect(shown).toContain("claude-work");
    expect(shown).toContain(code === "account_still_sharing" ? "cannot be turned off while" : "Nothing was rolled back");
    expect(toastError).not.toHaveBeenCalled();
  });

  it("reports a refusal to point a sharing account at another directory", async () => {
    const alerts = document.body.appendChild(document.createElement("div"));
    const alertsRoot = createRoot(alerts);
    onTestFinished(() => act(() => alertsRoot.unmount()));
    act(() => alertsRoot.render(<ErrorAlerts />));
    const { container } = mount();
    await openAccountMenu(container);
    await chooseItem("Edit");
    const dialog = document.querySelector('[role="dialog"]')!;
    const directory = [...dialog.querySelectorAll("input")].at(-1)!;
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(directory, "/home/dev/.claude-elsewhere");
      directory.dispatchEvent(new Event("input", { bubbles: true }));
    });
    request.mockRejectedValueOnce(new DaemonRequestError("busy", "account_still_sharing", { account: "claude-work", name: "claude-work" }));
    await act(async () => buttonNamed(dialog, "Save")!.click());
    expect(request).toHaveBeenCalledWith(expect.objectContaining({ type: "update_account", account: "claude-work", config_dir: "/home/dev/.claude-elsewhere" }));
    expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("cannot be turned off while");
  });
});

it.each([
  ["edit", false], ["edit", true],
  ["add", false], ["add", true],
  ["rename", false], ["rename", true],
  ["remove account", false], ["remove account", true],
  ["remove agent", false], ["remove agent", true],
] as const)("dismisses the %s form for navigation requesting Add agent=%s", async (form, addAgent) => {
  const { container, navigate } = mount();
  if (form === "add") {
    act(() => buttonNamed(container, "Add account")!.click());
  } else if (form === "remove agent") {
    await openTabMenu(container, "Claude Code");
    await chooseItem("Remove agent");
  } else {
    const row = rowOf(container, form === "rename" ? "default:claude" : "claude-work");
    const trigger = row.querySelector<HTMLButtonElement>('button[aria-label^="Actions"]')!;
    await act(async () => {
      trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
      trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
      trigger.click();
    });
    await chooseItem(form === "edit" ? "Edit" : form === "rename" ? "Rename" : "Remove");
  }
  expect(document.querySelector('[role="dialog"], [role="alertdialog"]')).not.toBeNull();
  navigate(addAgent);
  await act(async () => {});
  const dialogs = [...document.querySelectorAll('[role="dialog"], [role="alertdialog"]')];
  if (addAgent) {
    expect(dialogs).toHaveLength(1);
    expect(dialogs[0]!.textContent).toContain("Only an agent installed on this machine can be added.");
  } else {
    expect(dialogs).toHaveLength(0);
  }
});

it.each([false, true])("keeps an active login for navigation requesting Add agent=%s", async (addAgent) => {
  const login: AccountLogin = {
    agent: "claude", account: "claude-work", status: "waiting", authorization_url: null,
    device_code: null, opened_by_agent: false, failure: null,
  };
  accountLogins.push(login);
  const { navigate } = mount();
  await act(async () => {});
  const dialog = document.querySelector('[role="dialog"]')!;
  expect(dialog.textContent).toContain("Waiting for you to authorize");
  navigate(addAgent);
  await act(async () => {});
  expect(dialog.isConnected).toBe(true);
  expect(dialog.textContent).toContain("Waiting for you to authorize");
  expect(accountLogins).toEqual([login]);
  expect(request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "cancel_account_login" }));
});

it("lets Manage accounts recovery dismiss a stale editor and focus the real Settings section", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  onTestFinished(() => { document.documentElement.style.removeProperty("--breakpoint-docked"); });
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: media.startsWith("(min-width:"), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => { vi.unstubAllGlobals(); });
  let nonce = 1;
  const render = () => act(() => root.render(<><SettingsDialog navigation={{ nonce, section: "accounts", addAgent: false }} onClose={() => {}} /><ErrorAlerts /></>));
  const unregister = registerSettingsLauncher(() => { nonce += 1; render(); });
  onTestFinished(() => {
    unregister();
    act(() => {
      root.unmount();
      while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
    });
    container.remove();
  });
  render();
  await trackMountRefocus()();
  const row = rowOf(document, "claude-work");
  await act(async () => {
    const trigger = row.querySelector<HTMLButtonElement>('button[aria-label^="Actions"]')!;
    trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.click();
  });
  await chooseItem("Edit");
  await trackMountRefocus()();
  act(() => {
    settings.accounts = settings.accounts.filter((account) => account.id !== "claude-work");
    bus.notify();
  });
  request.mockRejectedValueOnce(new DaemonRequestError("Account no longer exists.", "unknown_account", {}));
  await act(async () => buttonNamed(document, "Save")!.click());
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Manage accounts…");
  act(() => buttonNamed(document, "Manage accounts…")!.click());
  await act(async () => {
    for (let frame = 0; frame < 4; frame++) await new Promise((resolve) => requestAnimationFrame(resolve));
  });
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
  const selected = document.querySelector('[role="tab"][aria-selected="true"]');
  expect(selected?.textContent).toContain("Agent accounts");
  expect(document.activeElement).toBe(selected);
});
