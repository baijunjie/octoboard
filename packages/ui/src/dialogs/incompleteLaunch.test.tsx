// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Account, Agent, Session } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { registerSettingsLauncher } from "../settings/settingsLauncher";
import { trackMountRefocus } from "./dialogTestSupport";
import { HandoffDialog } from "./HandoffDialog";
import { SessionDialog } from "./SessionDialog";
import { SwitchAccountDialog } from "./SwitchAccountDialog";
import { useConfirmIncompleteLaunch, type ConfirmLaunch } from "./useConfirmIncompleteLaunch";

vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The confirmation about an account whose last sharing change did not finish. */
const ASKING = '[role="alertdialog"]';

const half = (agent: Agent): Account => ({ id: `${agent}-half`, agent, name: "Half", config_dir: `/a/${agent}`, share_default_config: true, share_default_config_incomplete: true });
const complete = (agent: Agent): Account => ({ ...half(agent), share_default_config_incomplete: false });

/** A console whose new sessions of every agent open on that agent's `account`. */
const consoleOn = (accounts: Account[]) => consoleOf("c-1", "Main", Object.fromEntries(accounts.map((account) => [`${account.agent}_account_id`, account.id])));
const project = projectOf("p-1", "c-1", "Search API");

function render(accounts: Account[], ui: (daemon: Daemon) => React.ReactElement, sessions: Session[] = []) {
  const parent = consoleOn(accounts);
  const store = createStateStore({
    consoles: new Map([[parent.id, parent]]),
    projects: new Map([[project.id, project]]),
    sessions: new Map(sessions.map((session) => [session.id, session])),
  });
  store.setState({ settings: { ...store.getState().settings, accounts } });
  const request = vi.fn(async (body: { type: string }) => {
    if (body.type === "locate_conversation_to_resume") return { type: "conversation_located", location: { kind: "archived_session", session: "s-old" } };
    return body.type === "ack" ? { type: "ack" } : { type: "session_opened", session: sessionOf("s-new", parent.id, undefined, "", "idle") };
  });
  const daemon = { store, request, toastError: vi.fn(), toastNotice: vi.fn(), onToast: () => () => {}, dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "" } as unknown as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<DaemonProvider value={daemon}>{ui(daemon)}</DaemonProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { request, parent };
}

/** Presses the last button of that name, which is the topmost dialog's when the question stands over another. */
const press = (label: string) => act(async () => Array.from(document.body.querySelectorAll("button")).filter((button) => button.textContent === label).at(-1)!.click());
/** Closes the question with its close button, which is the only way out besides Escape and the two answers. */
const close = () => act(async () => Array.from(document.body.querySelectorAll<HTMLElement>(ASKING + ' button[aria-label="Close"]')).at(-1)!.click());
const settle = () => act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
const sent = (request: ReturnType<typeof render>["request"], type: string) => request.mock.calls.filter(([body]) => body.type === type);

/** Renders the hook with no dialog around its question and returns the `confirmLaunch` it hands out. */
function renderProbe(): ConfirmLaunch {
  let ask!: ConfirmLaunch;
  const Probe = () => {
    const { confirmLaunch, confirmation } = useConfirmIncompleteLaunch();
    ask = confirmLaunch;
    return confirmation ?? null;
  };
  render([half("claude")], () => <Probe />);
  return ask;
}
/** Sends Escape to the element holding focus. */
const escape = () => act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, key: "Escape" })));

const sessionDialog = (daemon: Daemon, onResume: (sessionId: string) => Promise<void> = async () => {}) => (
  <SessionDialog console={daemon.store.getState().consoles.get("c-1")!} project={project} binding={{ kind: "choose" }} sessions={[]} onClose={() => {}} onOpened={() => {}} onResume={onResume} />
);
const handoffDialog = () => (
  <HandoffDialog session={sessionOf("s-1", "c-1", project.id, "Fix search", "idle")} onClose={() => {}} onHandedOff={() => {}} />
);
const switching = sessionOf("s-1", "c-1", project.id, "Fix search", "idle");
const switchDialog = (onSwitchAccount: () => Promise<void>) => () => (
  <SwitchAccountDialog session={switching} account="claude-half" accountName="Half" onClose={() => {}} onSwitchAccount={onSwitchAccount} />
);

it("opening a new session on an incomplete account asks first, and sends the same request once confirmed", async () => {
  const { request } = render([half("claude")], sessionDialog);
  await press("Open");
  expect(sent(request, "open_session")).toHaveLength(0);
  expect(document.body.querySelector(ASKING)?.textContent).toContain("The last sharing change of this account did not finish");
  await press("Start anyway");
  await settle();
  expect(sent(request, "open_session")).toEqual([[expect.objectContaining({ type: "open_session", account: "claude-half" })]]);
});

it("closing the question about a new session sends nothing and leaves the dialog open", async () => {
  const { request } = render([half("claude")], sessionDialog);
  await press("Open");
  await close();
  await settle();
  expect(request).not.toHaveBeenCalled();
  expect(document.body.querySelector(ASKING)).toBeNull();
  expect(document.body.querySelector('[role="dialog"]')).not.toBeNull();
});

it("handing off onto an incomplete account asks first, sends nothing on close and the handoff on Confirm", async () => {
  const { request } = render([half("codex")], handoffDialog);
  await press("Hand off");
  expect(document.body.querySelector(ASKING)).not.toBeNull();
  await close();
  await settle();
  expect(sent(request, "hand_off_session")).toHaveLength(0);
  await press("Hand off");
  await press("Start anyway");
  await settle();
  expect(sent(request, "hand_off_session")).toEqual([[expect.objectContaining({ type: "hand_off_session", agent: "codex", account: "codex-half" })]]);
});

it("switching a session onto an incomplete account asks after the switch is confirmed, and switches only on Start anyway", async () => {
  const onSwitchAccount = vi.fn(async () => {});
  render([half("claude")], switchDialog(onSwitchAccount), [switching]);
  // A press that is not the pointer's leaves focus on the switch dialog's Confirm; the question opens
  // as that button turns pending, and in a browser focus has fallen to the page by then.
  document.body.querySelector<HTMLButtonElement>('button[type="submit"]')!.focus();
  const lose = (event: FocusEvent) => (event.target as HTMLElement).blur();
  document.addEventListener("focusin", lose, { once: true });
  await press("Switch");
  const settled = trackMountRefocus();
  expect(document.body.querySelectorAll(ASKING)).toHaveLength(2);
  await settled();
  expect(document.activeElement?.textContent).toBe("Start anyway");
  expect(onSwitchAccount).not.toHaveBeenCalled();
  await close();
  await settle();
  expect(onSwitchAccount).not.toHaveBeenCalled();
  await press("Switch");
  await press("Start anyway");
  await settle();
  expect(onSwitchAccount).toHaveBeenCalledExactlyOnceWith("s-1", "claude-half");
});

const switchedOnto = vi.fn(async () => {});

it.each([
  ["opening a new session", () => render([complete("claude")], sessionDialog), "Open", async (request: ReturnType<typeof render>["request"]) => sent(request, "open_session").length],
  ["handing off", () => render([complete("codex")], handoffDialog), "Hand off", async (request: ReturnType<typeof render>["request"]) => sent(request, "hand_off_session").length],
  ["switching", () => { switchedOnto.mockClear(); return render([complete("claude")], switchDialog(switchedOnto), [switching]); }, "Switch", async () => switchedOnto.mock.calls.length],
])("does not ask when %s onto a complete account", async (_name, mount, button, launches) => {
  const { request } = mount();
  await press(button);
  await settle();
  // The switch dialog is itself an alert dialog, so what is absent is the question's text.
  expect(document.body.textContent).not.toContain("did not finish");
  expect(await launches(request)).toBe(1);
});

const archived = sessionOf("s-old", "c-1", project.id, "Old work", "archived", { account_id: "claude-half" });

/** Opens the Resume tab of the new-session dialog and types the ID of the archived session, whose account is `half("claude")`. */
async function resumeArchived(onResume: (sessionId: string) => Promise<void>) {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  const rendered = render([half("claude")], (daemon) => sessionDialog(daemon, onResume), [archived]);
  await act(async () => Array.from(document.body.querySelectorAll<HTMLElement>('[role="tab"]')).find((tab) => tab.textContent === "Resume conversation")!.click());
  act(() => {
    const field = Array.from(document.body.querySelectorAll<HTMLInputElement>("input")).find((input) => input.labels?.[0]?.textContent === "Session ID")!;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(field, "11111111-2222-3333-4444-555555555555");
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => { await vi.advanceTimersByTimeAsync(300); });
  return rendered;
}

it("resuming an archived session of an incomplete account asks first, resumes nothing on close and resumes on Start anyway", async () => {
  const onResume = vi.fn(async () => {});
  await resumeArchived(onResume);
  await press("Reopen");
  expect(document.body.querySelector(ASKING)).not.toBeNull();
  expect(onResume).not.toHaveBeenCalled();
  await close();
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  expect(onResume).not.toHaveBeenCalled();
  expect(document.body.querySelector(ASKING)).toBeNull();
  await press("Reopen");
  expect(document.activeElement?.textContent).toBe("Start anyway");
  await press("Start anyway");
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  expect(onResume).toHaveBeenCalledExactlyOnceWith("s-old");
});

it("Escape in the question closes only the question, and Start anyway launches once without submitting the dialog under it", async () => {
  const { request } = render([half("claude")], sessionDialog);
  await press("Open");
  await escape();
  await settle();
  expect(document.body.querySelector(ASKING)).toBeNull();
  expect(document.body.querySelector('[role="dialog"]')).not.toBeNull();
  expect(request).not.toHaveBeenCalled();
  await press("Open");
  await press("Start anyway");
  await settle();
  expect(sent(request, "open_session")).toHaveLength(1);
});

it("a second question while one is open cancels the first, whose launch never runs", async () => {
  const ask = renderProbe();
  const first = vi.fn();
  const second = vi.fn();
  let firstSettled = false;
  act(() => void ask("claude-half", first).then(() => { firstSettled = true; }));
  act(() => void ask("claude-half", second));
  await settle();
  expect(firstSettled).toBe(true);
  expect(document.body.querySelectorAll(ASKING)).toHaveLength(1);
  await press("Start anyway");
  expect(first).not.toHaveBeenCalled();
  expect(second).toHaveBeenCalledOnce();
});

it("offers View in Settings and Start anyway only, with Start anyway focused and a close button", async () => {
  render([half("claude")], sessionDialog);
  await press("Open");
  const labels = Array.from(document.body.querySelectorAll(ASKING + " button")).map((button) => button.textContent).filter(Boolean);
  expect(labels).toEqual(["View in Settings", "Start anyway"]);
  expect(document.body.querySelector(ASKING + ' button[aria-label="Close"]')).not.toBeNull();
  expect(document.activeElement?.textContent).toBe("Start anyway");
});

it("View in Settings sends nothing, closes the question and the dialog under it, and opens Settings on the account's sharing switch", async () => {
  const open = vi.fn();
  onTestFinished(registerSettingsLauncher(open));
  const onSwitchAccount = vi.fn(async () => {});
  const onClose = vi.fn();
  const { request } = render([half("claude")], () => (
    <SwitchAccountDialog session={switching} account="claude-half" accountName="Half" onClose={onClose} onSwitchAccount={onSwitchAccount} />
  ), [switching]);
  await press("Switch");
  expect(document.body.querySelectorAll(ASKING)).toHaveLength(2);
  await press("View in Settings");
  await settle();
  expect(document.body.querySelectorAll(ASKING)).toHaveLength(1);
  expect(onClose).toHaveBeenCalledOnce();
  expect(open).toHaveBeenCalledExactlyOnceWith("accounts", { account: "claude-half" });
  expect(onSwitchAccount).not.toHaveBeenCalled();
  expect(request).not.toHaveBeenCalled();
});

it("View in Settings on a new session's question closes the session dialog and settles the launch as cancelled", async () => {
  const open = vi.fn();
  onTestFinished(registerSettingsLauncher(open));
  const onClose = vi.fn();
  const { request } = render([half("claude")], (daemon) => (
    <SessionDialog console={daemon.store.getState().consoles.get("c-1")!} project={project} binding={{ kind: "choose" }} sessions={[]} onClose={onClose} onOpened={() => {}} onResume={async () => {}} />
  ));
  await press("Open");
  await press("View in Settings");
  await settle();
  expect(onClose).toHaveBeenCalledOnce();
  expect(open).toHaveBeenCalledExactlyOnceWith("accounts", { account: "claude-half" });
  expect(sent(request, "open_session")).toHaveLength(0);
});

it("View in Settings from a question with no dialog around it cancels the launch and opens Settings", async () => {
  const open = vi.fn();
  onTestFinished(registerSettingsLauncher(open));
  const ask = renderProbe();
  const launch = vi.fn();
  let settled = false;
  act(() => void ask("claude-half", launch).then(() => { settled = true; }));
  await press("View in Settings");
  await settle();
  expect(settled).toBe(true);
  expect(launch).not.toHaveBeenCalled();
  expect(open).toHaveBeenCalledExactlyOnceWith("accounts", { account: "claude-half" });
  expect(document.body.querySelector(ASKING)).toBeNull();
});

it("Escape in the question settles the launch as cancelled and sends nothing", async () => {
  const ask = renderProbe();
  const launch = vi.fn();
  let settled = false;
  act(() => void ask("claude-half", launch).then(() => { settled = true; }));
  await escape();
  await settle();
  expect(settled).toBe(true);
  expect(launch).not.toHaveBeenCalled();
  expect(document.body.querySelector(ASKING)).toBeNull();
});

it("Escape on the question over the switch dialog closes only the question and the switch dialog stays", async () => {
  const onSwitchAccount = vi.fn(async () => {});
  render([half("claude")], switchDialog(onSwitchAccount), [switching]);
  await press("Switch");
  expect(document.body.querySelectorAll(ASKING)).toHaveLength(2);
  await escape();
  await settle();
  expect(document.body.querySelectorAll(ASKING)).toHaveLength(1);
  expect(onSwitchAccount).not.toHaveBeenCalled();
});
