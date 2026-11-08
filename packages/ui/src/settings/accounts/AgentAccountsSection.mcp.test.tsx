// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import type { AccountMcpInventory, LocalMcpServer, McpServerAuthOutcome } from "../../protocol";
import { errorAlerts } from "../../components/alerts/errorAlerts";
import { trackMountRefocus } from "../../components/dialog/dialogTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS, FIRST_WRITE_DELAY_MS } from "../../components/announce/StatusAnnouncer";
import { windowAttention } from "../../components/announce/windowTestSupport";
import { press } from "../../components/hint/tooltipTestSupport";
import { ActionableError } from "../../errors/actionableError";
import { CodedError } from "../../errors/codedError";
import { isolate } from "../../i18n/isolate";
import { readMcpInventory, request } from "./agentAccountsSectionTestState";
import { buttonNamed, mount, openDetailsOf } from "./agentAccountsSectionTestSupport";

vi.mock("../../components/icons/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

const inventoryOf = (patch: Partial<AccountMcpInventory> = {}): AccountMcpInventory => ({
  agent: "claude",
  account: "claude-work",
  read_at: Date.now(),
  connectors: [],
  servers: [],
  unavailable: null,
  ...patch,
});

/** Answers the next inventory reading by hand: `answer` resolves it, `fail` rejects it. */
function nextReading() {
  let answer!: (inventory: AccountMcpInventory) => void;
  let fail!: (error: Error) => void;
  readMcpInventory.mockImplementationOnce(() => new Promise((resolve, reject) => {
    answer = (inventory) => resolve({ type: "account_mcp_inventory_read", id: null, inventory });
    fail = reject;
  }));
  return {
    answer: (inventory: AccountMcpInventory) => act(async () => answer(inventory)),
    fail: (error: Error) => act(async () => fail(error)),
  };
}

const serverOf = (patch: Partial<LocalMcpServer> = {}): LocalMcpServer => ({
  name: "jira", origin: "https://jira.example.com/mcp", plugin: null, sign_in: "needs_sign_in", runtime: "unknown", runtime_message: null, tool_count: null, oauth_sign_in: true, ...patch,
});

/** Holds the next sign-in or sign-out request, which `end` answers with an outcome and `refuse` with a refusal. */
function nextAuth() {
  let end!: (outcome: McpServerAuthOutcome) => void;
  let refuse!: (error: Error) => void;
  request.mockImplementationOnce(() => new Promise((resolve, reject) => {
    end = (outcome) => resolve({ type: "account_mcp_server_auth_ended", id: null, outcome });
    refuse = reject;
  }));
  return {
    end: (outcome: McpServerAuthOutcome) => act(async () => end(outcome)),
    refuse: (error: Error) => act(async () => refuse(error)),
  };
}

/** A Codex account's details with `servers` read, ready for a sign-in or sign-out. */
async function openWithServers(servers: LocalMcpServer[]) {
  const reading = nextReading();
  const { container } = mount("Codex");
  const dialog = await openDetailsOf(container, "codex-work");
  await reading.answer(inventoryOf({ agent: "codex", account: "codex-work", servers }));
  return dialog;
}

/** The dialog's part named by its "Connectors and MCP" heading. */
const section = (dialog: HTMLElement) => [...dialog.querySelectorAll<HTMLElement>("section[aria-labelledby]")]
  .find((part) => document.getElementById(part.getAttribute("aria-labelledby")!)?.textContent === "Connectors and MCP");

const CONNECTION_CLOSED = () => new CodedError("The daemon control connection closed.", "connection_closed", {});

it("reads the inventory when a Claude Code account's details open, showing that it reads until the answer", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openDetailsOf(container, "claude-work");
  expect(readMcpInventory.mock.calls).toEqual([[{ type: "read_account_mcp_inventory", agent: "claude", account: "claude-work", refresh: false }]]);
  expect(section(dialog)!.textContent).toContain("Reading connectors and MCP servers…");
  await reading.answer(inventoryOf());
  expect(section(dialog)!.textContent).toContain("No platform connectors");
  expect(section(dialog)!.textContent).toContain("No local MCP servers");
  expect(section(dialog)!.textContent).toContain("Updated just now");
});

it("shows each connector and server with its state, and leads to the platform's connector settings", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openDetailsOf(container, "claude-work");
  await reading.answer(inventoryOf({
    connectors: [
      { id: "claude.ai Gmail", name: "Gmail", state: "connected", plugins: [] },
      { id: "claude.ai Slack", name: "Slack", state: "not_connected", plugins: [] },
    ],
    servers: [
      { name: "linear", origin: "https://mcp.linear.app/mcp", plugin: null, sign_in: "needs_sign_in", runtime: "unknown", runtime_message: null, tool_count: null, oauth_sign_in: false },
      { name: "files", origin: null, plugin: "toolbox", sign_in: "not_needed", runtime: "failed", runtime_message: "spawn ENOENT", tool_count: null, oauth_sign_in: false },
      { name: "notes", origin: null, plugin: null, sign_in: "not_needed", runtime: "running", runtime_message: null, tool_count: 3, oauth_sign_in: false },
    ],
  }));
  const text = section(dialog)!.textContent!;
  expect(text).toMatch(/Gmail.*Connected.*Slack.*Not connected/);
  expect(text).toMatch(/linear.*Needs sign-in.*https:\/\/mcp\.linear\.app\/mcp/);
  expect(text).toContain("To sign in, run /mcp in a session of this account.");
  expect(text).toMatch(/files.*Failed.*From plugin toolbox/);
  expect(text).toMatch(/notes.*Running.*3 tools/);
  // The failed server's mark holds what the agent said, not the entry.
  expect(text).not.toContain("spawn ENOENT");
  expect(buttonNamed(dialog, "Failed to connect")).toBeDefined();
  const link = dialog.querySelector<HTMLAnchorElement>("a[data-external-link]")!;
  expect(link.textContent).toBe("Manage on claude.ai");
  expect(link.getAttribute("href")).toBe("https://claude.ai/customize/connectors");
  expect(text).toContain("To connect or disconnect connectors, log in to claude.ai with the account this one uses.");
});

it("leads a Codex account to ChatGPT and gives no session command for a server that needs sign-in", async () => {
  const reading = nextReading();
  const { container } = mount("Codex");
  const dialog = await openDetailsOf(container, "codex-work");
  await reading.answer(inventoryOf({
    agent: "codex",
    account: "codex-work",
    connectors: [{ id: "github", name: "GitHub", state: "connected", plugins: ["github", "devtools"] }],
    servers: [{ name: "jira", origin: "https://jira.example.com/mcp", plugin: null, sign_in: "needs_sign_in", runtime: "unknown", runtime_message: null, tool_count: null, oauth_sign_in: false }],
  }));
  expect(dialog.querySelector("a[data-external-link]")!.getAttribute("href")).toBe("https://chatgpt.com/plugins");
  expect(section(dialog)!.textContent).toContain("From plugins github and devtools");
  expect(section(dialog)!.textContent).not.toContain("To sign in");
});

it("rereads with the refresh flag, keeping the last reading while it runs and ignoring presses until it answers", async () => {
  const first = nextReading();
  const { container } = mount();
  const dialog = await openDetailsOf(container, "claude-work");
  await first.answer(inventoryOf({ connectors: [{ id: "a", name: "Gmail", state: "connected", plugins: [] }] }));
  const second = nextReading();
  const refresh = buttonNamed(dialog, "Refresh connectors and MCP servers")!;
  press(refresh);
  press(refresh);
  expect(readMcpInventory.mock.calls.slice(1)).toEqual([[{ type: "read_account_mcp_inventory", agent: "claude", account: "claude-work", refresh: true }]]);
  expect(refresh.getAttribute("aria-busy")).toBe("true");
  expect(section(dialog)!.textContent).toContain("Gmail");
  await second.answer(inventoryOf({ connectors: [{ id: "b", name: "Drive", state: "connected", plugins: [] }] }));
  expect(section(dialog)!.textContent).toContain("Drive");
  expect(section(dialog)!.textContent).not.toContain("Gmail");
  expect(refresh.getAttribute("aria-busy")).toBeNull();
});

it.each<[AccountMcpInventory["unavailable"] & object, string, string, boolean]>([
  [{ reason: "not_signed_in", detail: null }, "Account not logged in", "Log in to the account, then try again.", true],
  [{ reason: "cli_missing", detail: null }, "Claude Code is not available", "Its command-line tool could not be found or started.", true],
  [{ reason: "timed_out", detail: null }, "Reading timed out", "Claude Code did not answer in time.", true],
  [{ reason: "unsupported_agent", detail: null }, "Not supported", "Claude Code cannot list its MCP servers or connectors.", false],
  [{ reason: "failed", detail: "claude exited with status 1" }, "Could not read connectors and MCP servers", "Try again in a moment.", true],
])("words a reading that failed as %j, offering Try again where it can change the answer", async (unavailable, title, line, retries) => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openDetailsOf(container, "claude-work");
  await reading.answer(inventoryOf({ unavailable }));
  const text = section(dialog)!.textContent!;
  expect(text).toContain(title);
  expect(text).toContain(line);
  expect(text).not.toContain("Updated");
  expect(!!buttonNamed(section(dialog)!, "Try again")).toBe(retries);
  expect(!!buttonNamed(section(dialog)!, "Details")).toBe(unavailable.detail !== null);
  if (retries) {
    nextReading();
    press(buttonNamed(section(dialog)!, "Try again")!);
    expect(readMcpInventory.mock.calls.at(-1)![0].refresh).toBe(true);
  }
});

it("hands focus to Refresh when Try again brings the inventory in place of the failure", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openDetailsOf(container, "claude-work");
  await reading.answer(inventoryOf({ unavailable: { reason: "timed_out", detail: null } }));
  const retry = nextReading();
  const tryAgain = buttonNamed(section(dialog)!, "Try again")!;
  act(() => tryAgain.focus());
  press(tryAgain);
  await retry.answer(inventoryOf());
  expect(document.activeElement).toBe(buttonNamed(dialog, "Refresh connectors and MCP servers"));
});

it("shows a refused reading as a failure with Try again", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openDetailsOf(container, "claude-work");
  await reading.fail(CONNECTION_CLOSED());
  expect(section(dialog)!.textContent).toContain("Could not read connectors and MCP servers");
  expect(buttonNamed(section(dialog)!, "Try again")).toBeDefined();
});

it("has no such section, and reads nothing, for a provider that cannot list them", async () => {
  const { container } = mount("Grok Build");
  const dialog = await openDetailsOf(container, "grok-work");
  expect(section(dialog)).toBeUndefined();
  expect(readMcpInventory).not.toHaveBeenCalled();
});

it("offers Sign in or Sign out only for a server the agent signs in to with OAuth", async () => {
  const dialog = await openWithServers([
    serverOf({ name: "jira" }),
    serverOf({ name: "wiki", sign_in: "signed_in" }),
    serverOf({ name: "bearer", sign_in: "needs_sign_in", oauth_sign_in: false }),
    serverOf({ name: "notes", sign_in: "not_needed", oauth_sign_in: false }),
  ]);
  const names = [...section(dialog)!.querySelectorAll("button")].map((button) => button.getAttribute("aria-label"));
  expect(names).toContain(`Sign in to ${isolate("jira")}`);
  expect(names).toContain(`Sign out of ${isolate("wiki")}`);
  expect(names.filter((name) => /^Sign (in|out)/.test(name ?? ""))).toHaveLength(2);
});

it("offers no sign-in on a platform connector or on a Claude Code server", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openDetailsOf(container, "claude-work");
  await reading.answer(inventoryOf({
    connectors: [{ id: "a", name: "Gmail", state: "not_connected", plugins: [] }],
    servers: [serverOf({ oauth_sign_in: false })],
  }));
  expect(buttonNamed(section(dialog)!, `Sign in to ${isolate("jira")}`)).toBeUndefined();
  expect([...section(dialog)!.querySelectorAll("button")].map((button) => button.textContent)).not.toContain("Sign in");
});

it("signs in, showing the wait for the browser with Cancel until it ends, then reads the inventory again", async () => {
  const dialog = await openWithServers([serverOf(), serverOf({ name: "wiki" })]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  expect(request.mock.calls.at(-1)![0]).toEqual({ type: "sign_in_account_mcp_server", agent: "codex", account: "codex-work", server: "jira" });
  const text = () => section(dialog)!.textContent!;
  expect(text()).toContain("Waiting for the sign-in in your browser…");
  expect(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!.getAttribute("aria-busy")).toBe("true");
  expect(buttonNamed(dialog, `Cancel signing in to ${isolate("jira")}`)).toBeDefined();
  // One run per server, and the other servers stay usable.
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  expect(request).toHaveBeenCalledTimes(1);
  expect(buttonNamed(dialog, `Sign in to ${isolate("wiki")}`)!.hasAttribute("disabled")).toBe(false);
  const reread = nextReading();
  await auth.end({ status: "succeeded", detail: null });
  expect(readMcpInventory.mock.calls.at(-1)![0]).toEqual({ type: "read_account_mcp_inventory", agent: "codex", account: "codex-work", refresh: true });
  expect(text()).not.toContain("Waiting for the sign-in");
  expect(buttonNamed(dialog, `Cancel signing in to ${isolate("jira")}`)).toBeUndefined();
  // The old state stays, its control pending, until the new reading answers.
  expect(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!.getAttribute("aria-busy")).toBe("true");
  await reread.answer(inventoryOf({ agent: "codex", account: "codex-work", servers: [serverOf({ sign_in: "signed_in" }), serverOf({ name: "wiki" })] }));
  expect(buttonNamed(dialog, `Sign out of ${isolate("jira")}`)!.getAttribute("aria-busy")).toBeNull();
  expect(text()).toContain("Signed in");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("reads again after the reading that is under way when a sign-in succeeds", async () => {
  const dialog = await openWithServers([serverOf()]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  const under = nextReading();
  press(buttonNamed(dialog, "Refresh connectors and MCP servers")!);
  await auth.end({ status: "succeeded", detail: null });
  const readsBefore = readMcpInventory.mock.calls.length;
  const after = nextReading();
  await under.answer(inventoryOf({ agent: "codex", servers: [serverOf()] }));
  expect(readMcpInventory.mock.calls.length).toBe(readsBefore + 1);
  expect(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!.getAttribute("aria-busy")).toBe("true");
  await after.answer(inventoryOf({ agent: "codex", servers: [serverOf({ sign_in: "signed_in" })] }));
  expect(buttonNamed(dialog, `Sign out of ${isolate("jira")}`)).toBeDefined();
});

it("sends the cancel of a sign-in and shows nothing once it answers as cancelled", async () => {
  const dialog = await openWithServers([serverOf()]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  request.mockResolvedValueOnce({ type: "ack", id: null });
  press(buttonNamed(dialog, `Cancel signing in to ${isolate("jira")}`)!);
  expect(request.mock.calls.at(-1)![0]).toEqual({ type: "cancel_account_mcp_sign_in", agent: "codex", account: "codex-work", server: "jira" });
  const rereads = readMcpInventory.mock.calls.length;
  await auth.end({ status: "cancelled", detail: null });
  expect(section(dialog)!.textContent).not.toContain("Waiting for the sign-in");
  expect(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!.getAttribute("aria-busy")).toBeNull();
  expect(readMcpInventory.mock.calls.length).toBe(rereads);
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("raises a failed sign-in as an error alert with what the CLI said behind its details", async () => {
  const dialog = await openWithServers([serverOf()]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  await auth.end({ status: "failed", detail: "codex mcp login: invalid_client" });
  const [alert] = errorAlerts.getSnapshot();
  expect(alert.title).toBe("Could not sign in to the MCP server");
  expect(alert.message).toBe(`The sign-in to ${isolate("jira")} did not finish.`);
  expect((alert.error as { detail?: string }).detail).toBe("codex mcp login: invalid_client");
  expect((alert.error as ActionableError).recovery).toBe("mcpSignIn");
  expect(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!.getAttribute("aria-busy")).toBeNull();
});

it("words a sign-in that timed out as not finished in the browser in time", async () => {
  const dialog = await openWithServers([serverOf()]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  await auth.end({ status: "timed_out", detail: null });
  const [alert] = errorAlerts.getSnapshot();
  expect(alert.title).toBe("Could not sign in to the MCP server");
  expect(alert.message).toBe(`The sign-in to ${isolate("jira")} was not finished in the browser in time.`);
  expect((alert.error as ActionableError).recovery).toBe("mcpSignInTimedOut");
});

it("raises a refused sign-in as an error alert titled for the sign-in", async () => {
  const dialog = await openWithServers([serverOf()]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  await auth.refuse(new CodedError("Another sign-in or sign-out of this server is already running.", "mcp_server_auth_busy", { agent: "codex", server: "jira" }));
  const [alert] = errorAlerts.getSnapshot();
  expect(alert.title).toBe("Could not sign in to the MCP server");
  expect(alert.error).toMatchObject({ code: "mcp_server_auth_busy" });
  expect(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!.getAttribute("aria-busy")).toBeNull();
});

it("signs out with the control pending until the answer, then reads again", async () => {
  const dialog = await openWithServers([serverOf({ sign_in: "signed_in" })]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign out of ${isolate("jira")}`)!);
  expect(request.mock.calls.at(-1)![0]).toEqual({ type: "sign_out_account_mcp_server", agent: "codex", account: "codex-work", server: "jira" });
  expect(buttonNamed(dialog, `Sign out of ${isolate("jira")}`)!.getAttribute("aria-busy")).toBe("true");
  expect(section(dialog)!.textContent).not.toContain("Waiting for the sign-in");
  expect(buttonNamed(dialog, `Cancel signing in to ${isolate("jira")}`)).toBeUndefined();
  const reread = nextReading();
  await auth.end({ status: "succeeded", detail: null });
  await reread.answer(inventoryOf({ agent: "codex", servers: [serverOf()] }));
  expect(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)).toBeDefined();
});

it("raises a failed sign-out as an error alert titled for the sign-out", async () => {
  const dialog = await openWithServers([serverOf({ sign_in: "signed_in" })]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign out of ${isolate("jira")}`)!);
  await auth.end({ status: "failed", detail: "boom" });
  const [alert] = errorAlerts.getSnapshot();
  expect(alert.title).toBe("Could not sign out of the MCP server");
  expect(alert.message).toBe(`The sign-out of ${isolate("jira")} did not finish.`);
  expect((alert.error as ActionableError).recovery).toBe("mcpSignOut");
  expect(buttonNamed(dialog, `Sign out of ${isolate("jira")}`)!.getAttribute("aria-busy")).toBeNull();
});

const closeDialog = (dialog: HTMLElement) => act(async () => buttonNamed(dialog, "Close")!.click());

it("raises a sign-in that fails after the dialog has closed", async () => {
  const dialog = await openWithServers([serverOf()]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  await closeDialog(dialog);
  await auth.end({ status: "failed", detail: null });
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not sign in to the MCP server"]);
});

it("reads nothing again for a sign-in that succeeds after the dialog has closed", async () => {
  const dialog = await openWithServers([serverOf()]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  await closeDialog(dialog);
  const reads = readMcpInventory.mock.calls.length;
  await auth.end({ status: "succeeded", detail: null });
  expect(readMcpInventory.mock.calls.length).toBe(reads);
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it.each([
  ["no sign-in running any more", new CodedError("No sign-in is running for this server.", "mcp_sign_in_not_running", { agent: "codex", server: "jira" }), []],
  ["the connection closed", CONNECTION_CLOSED(), ["Could not cancel the sign-in"]],
])("takes a cancel refused with %s back, raising it unless the sign-in had already ended", async (_, error, titles) => {
  const dialog = await openWithServers([serverOf()]);
  nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  let refuse!: (error: Error) => void;
  request.mockImplementationOnce(() => new Promise((_resolve, reject) => { refuse = reject; }));
  const cancel = () => buttonNamed(dialog, `Cancel signing in to ${isolate("jira")}`)!;
  press(cancel());
  expect(cancel().getAttribute("aria-busy")).toBe("true");
  await act(async () => refuse(error));
  expect(cancel().getAttribute("aria-busy")).toBeNull();
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(titles);
});

it("raises a cancel refused after the dialog has closed", async () => {
  const dialog = await openWithServers([serverOf()]);
  nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  let refuse!: (error: Error) => void;
  request.mockImplementationOnce(() => new Promise((_resolve, reject) => { refuse = reject; }));
  press(buttonNamed(dialog, `Cancel signing in to ${isolate("jira")}`)!);
  await closeDialog(dialog);
  await act(async () => refuse(CONNECTION_CLOSED()));
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not cancel the sign-in"]);
});

it("hands focus on Cancel to the row's Sign in when the sign-in ends", async () => {
  const dialog = await openWithServers([serverOf()]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  request.mockResolvedValueOnce({ type: "ack", id: null });
  const cancel = buttonNamed(dialog, `Cancel signing in to ${isolate("jira")}`)!;
  act(() => cancel.focus());
  press(cancel);
  await auth.end({ status: "cancelled", detail: null });
  expect(document.activeElement).toBe(buttonNamed(dialog, `Sign in to ${isolate("jira")}`));
});

it("announces a sign-in that succeeded, a focus move's delay after it", async () => {
  windowAttention();
  const dialog = await openWithServers([serverOf()]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  nextReading();
  // The status region's first-write delay was armed on the real clock as it mounted (writing-automated-tests.md).
  await act(() => new Promise((resolve) => setTimeout(resolve, FIRST_WRITE_DELAY_MS)));
  fakeTimers();
  await auth.end({ status: "succeeded", detail: null });
  await act(async () => vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS - 1));
  expect(section(dialog)!.textContent).not.toContain("Signed in to");
  await act(async () => vi.advanceTimersByTime(1));
  expect(section(dialog)!.textContent).toContain(`Signed in to ${isolate("jira")}.`);
});

/** Fakes the clock for the rest of the test. */
function fakeTimers() {
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });
}

const alertRole = (dialog: HTMLElement) => section(dialog)!.querySelector("[role=alert]");

it("announces a first failure that answers right after the dialog opened a focus move's delay later, and a later one at once", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openDetailsOf(container, "claude-work");
  fakeTimers();
  await reading.fail(CONNECTION_CLOSED());
  expect(section(dialog)!.textContent).toContain("Could not read connectors and MCP servers");
  expect(alertRole(dialog)).toBeNull();
  await act(async () => vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  expect(alertRole(dialog)).not.toBeNull();
  const retry = nextReading();
  press(buttonNamed(section(dialog)!, "Try again")!);
  await retry.answer(inventoryOf({ unavailable: { reason: "timed_out", detail: null } }));
  expect(alertRole(dialog)).not.toBeNull();
});

it("hands focus to the new Alert's Try again when one failure's Alert replaces another's", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openDetailsOf(container, "claude-work");
  await reading.fail(CONNECTION_CLOSED());
  const retry = nextReading();
  const tryAgain = buttonNamed(section(dialog)!, "Try again")!;
  act(() => tryAgain.focus());
  press(tryAgain);
  await retry.answer(inventoryOf({ unavailable: { reason: "timed_out", detail: null } }));
  expect(section(dialog)!.textContent).toContain("Reading timed out");
  expect(document.activeElement).toBe(buttonNamed(section(dialog)!, "Try again"));
});

it("hands focus to Refresh when an Alert with no Try again replaces the one that held it", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openDetailsOf(container, "claude-work");
  await reading.fail(CONNECTION_CLOSED());
  const retry = nextReading();
  const tryAgain = buttonNamed(section(dialog)!, "Try again")!;
  act(() => tryAgain.focus());
  press(tryAgain);
  await retry.answer(inventoryOf({ unavailable: { reason: "unsupported_agent", detail: null } }));
  expect(document.activeElement).toBe(buttonNamed(dialog, "Refresh connectors and MCP servers"));
});

it("hands focus on a server's button to the Alert's Try again when the rereading after a sign-in fails", async () => {
  const dialog = await openWithServers([serverOf()]);
  const auth = nextAuth();
  const signIn = buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!;
  act(() => signIn.focus());
  press(signIn);
  const reread = nextReading();
  await auth.end({ status: "succeeded", detail: null });
  await reread.fail(CONNECTION_CLOSED());
  expect(document.activeElement).toBe(buttonNamed(section(dialog)!, "Try again"));
});

it("leaves focus on the page alone when a reading answers with nothing removed from under it", async () => {
  const first = nextReading();
  const { container } = mount();
  const mountRefocusSettled = trackMountRefocus();
  const dialog = await openDetailsOf(container, "claude-work");
  await first.answer(inventoryOf());
  await mountRefocusSettled();
  const second = nextReading();
  press(buttonNamed(dialog, "Refresh connectors and MCP servers")!);
  act(() => (document.activeElement as HTMLElement | null)?.blur());
  const before = document.activeElement;
  await second.fail(CONNECTION_CLOSED());
  expect(document.activeElement).toBe(before);
  expect(document.activeElement).not.toBe(buttonNamed(section(dialog)!, "Try again"));
});
