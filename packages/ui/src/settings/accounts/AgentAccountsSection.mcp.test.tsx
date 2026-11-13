// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import type { AccountMcpInventory, LocalMcpServer, McpServerAuthOutcome } from "../../protocol";
import { errorAlerts } from "../../components/alerts/errorAlerts";
import { framesHeld, trackMountRefocus } from "../../components/dialog/dialogTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS, FIRST_WRITE_DELAY_MS } from "../../components/announce/StatusAnnouncer";
import { windowAttention } from "../../components/announce/windowTestSupport";
import { press } from "../../components/hint/tooltipTestSupport";
import { ActionableError } from "../../errors/actionableError";
import { CodedError } from "../../errors/codedError";
import { isolate } from "../../i18n/isolate";
import { readMcpInventory, request } from "./agentAccountsSectionTestState";
import { buttonNamed, mount, openMcpOf } from "./agentAccountsSectionTestSupport";
import { fakeTimers } from "../../test/clock";
import { READING_CEILING_MS, SLOW_READING_MS } from "./useAccountMcpInventory";

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

/** A Codex account's Connectors and MCP dialog with `servers` read, ready for a sign-in or sign-out. */
async function openWithServers(servers: LocalMcpServer[]) {
  const reading = nextReading();
  const { container } = mount("Codex");
  const dialog = await openMcpOf(container, "codex-work");
  await reading.answer(inventoryOf({ agent: "codex", account: "codex-work", servers }));
  return dialog;
}

const REFRESH = "Refresh connectors and MCP servers";

/** The table of a group, found by the heading that labels it. */
function tableOf(dialog: ParentNode, heading: string): HTMLElement {
  const title = [...dialog.querySelectorAll("h3")].find((candidate) => candidate.textContent === heading)!;
  return dialog.querySelector<HTMLElement>(`table[aria-labelledby="${title.id}"]`)!;
}
const headersOf = (table: HTMLElement) => [...table.querySelectorAll('[role="columnheader"]')].map((header) => header.textContent);
/** The text of a cell, without the separator a screen reader hears before a badge. */
const textOf = (cell: Element) => cell.textContent!.replace(/^, /, "");
/** The row named `name`, as a list of its cells' texts: the row header first. */
function rowOf(table: HTMLElement, name: string) {
  const row = [...table.querySelectorAll<HTMLElement>('tbody [role="row"]')].find((candidate) => candidate.querySelector('[role="rowheader"]')?.textContent === name)!;
  const cells = [...row.querySelectorAll<HTMLElement>('[role="rowheader"], [role="gridcell"]')];
  return { row, cells, texts: cells.map(textOf) };
}
const spinners = (dialog: ParentNode) => dialog.querySelectorAll('[data-slot="spinner"]').length;

const CONNECTION_CLOSED = () => new CodedError("The daemon control connection closed.", "connection_closed", {});

it("reads the inventory when a Claude Code account's Connectors and MCP dialog opens, showing that it reads until the answer", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  expect(readMcpInventory.mock.calls).toEqual([[{ type: "read_account_mcp_inventory", agent: "claude", account: "claude-work", refresh: false }]]);
  expect(dialog.textContent).toContain("Reading connectors and MCP servers…");
  await reading.answer(inventoryOf());
  expect(dialog.textContent).toContain("No platform connectors");
  expect(dialog.textContent).toContain("No local MCP servers");
  expect(dialog.textContent).toContain("Updated just now");
});

it("names the account in a dialog titled Connectors and MCP, and reads once as it opens", async () => {
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  expect(dialog.querySelector("h2")?.textContent).toBe("Connectors and MCP");
  expect(dialog.textContent).toContain("claude-work");
  expect(readMcpInventory).toHaveBeenCalledTimes(1);
});

it("shows each connector and server with its state, and leads to the platform's connector settings", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
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
  const text = dialog.textContent!;
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
  const dialog = await openMcpOf(container, "codex-work");
  await reading.answer(inventoryOf({
    agent: "codex",
    account: "codex-work",
    connectors: [{ id: "github", name: "GitHub", state: "connected", plugins: ["github", "devtools"] }],
    servers: [{ name: "jira", origin: "https://jira.example.com/mcp", plugin: null, sign_in: "needs_sign_in", runtime: "unknown", runtime_message: null, tool_count: null, oauth_sign_in: false }],
  }));
  expect(dialog.querySelector("a[data-external-link]")!.getAttribute("href")).toBe("https://chatgpt.com/plugins");
  expect(dialog.textContent).toContain("From plugins github and devtools");
  expect(dialog.textContent).not.toContain("To sign in");
});

it("shows a Grok Build account's servers with no connector group, connector link or sign-in action", async () => {
  const reading = nextReading();
  const { container } = mount("Grok Build");
  const dialog = await openMcpOf(container, "grok-work");
  await reading.answer(inventoryOf({
    agent: "grok",
    account: "grok-work",
    servers: [
      serverOf({ name: "notes", origin: null, sign_in: "not_needed", runtime: "running", tool_count: 2, oauth_sign_in: false }),
      serverOf({ name: "linear", origin: "https://mcp.linear.app/mcp", sign_in: "needs_sign_in", oauth_sign_in: false }),
    ],
  }));
  const text = dialog.textContent!;
  expect(text).toMatch(/notes.*Running.*2 tools/);
  expect(text).toMatch(/linear.*Needs sign-in.*https:\/\/mcp\.linear\.app\/mcp/);
  expect(text).not.toContain("Platform connectors");
  expect(text).not.toContain("No platform connectors");
  expect(dialog.querySelector("a[data-external-link]")).toBeNull();
  expect(buttonNamed(dialog, "Sign in")).toBeUndefined();
  expect(buttonNamed(dialog, "Sign out")).toBeUndefined();
});

it("rereads with the refresh flag, keeping the last reading while it runs and ignoring presses until it answers", async () => {
  const first = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await first.answer(inventoryOf({ connectors: [{ id: "a", name: "Gmail", state: "connected", plugins: [] }] }));
  const second = nextReading();
  const refresh = buttonNamed(dialog, "Refresh connectors and MCP servers")!;
  expect(refresh.querySelector("svg.lucide-refresh-cw")).not.toBeNull();
  expect(refresh.querySelector("svg")!.classList.contains("motion-safe:animate-spin-slow")).toBe(false);
  press(refresh);
  press(refresh);
  const icon = refresh.querySelector("svg.lucide-refresh-cw")!;
  expect(icon).not.toBeNull();
  expect(icon.getAttribute("aria-hidden")).toBe("true");
  expect(icon.classList.contains("motion-safe:animate-spin-slow")).toBe(true);
  expect(spinners(refresh)).toBe(0);
  expect(readMcpInventory.mock.calls.slice(1)).toEqual([[{ type: "read_account_mcp_inventory", agent: "claude", account: "claude-work", refresh: true }]]);
  expect(refresh.getAttribute("aria-busy")).toBe("true");
  expect(refresh.classList.contains("[--disabled-opacity:1]")).toBe(true);
  expect(dialog.textContent).toContain("Gmail");
  await second.answer(inventoryOf({ connectors: [{ id: "b", name: "Drive", state: "connected", plugins: [] }] }));
  expect(dialog.textContent).toContain("Drive");
  expect(dialog.textContent).not.toContain("Gmail");
  expect(refresh.getAttribute("aria-busy")).toBeNull();
  expect(refresh.classList.contains("[--disabled-opacity:1]")).toBe(false);
  expect(refresh.querySelector("svg.lucide-refresh-cw")).not.toBeNull();
  expect(refresh.querySelector("svg")!.classList.contains("motion-safe:animate-spin-slow")).toBe(false);
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
  const dialog = await openMcpOf(container, "claude-work");
  await reading.answer(inventoryOf({ unavailable }));
  const text = dialog.textContent!;
  expect(text).toContain(title);
  expect(text).toContain(line);
  expect(text).not.toContain("Updated");
  expect(!!buttonNamed(dialog, "Try again")).toBe(retries);
  expect(!!buttonNamed(dialog, "Details")).toBe(unavailable.detail !== null);
  if (retries) {
    nextReading();
    press(buttonNamed(dialog, "Try again")!);
    expect(readMcpInventory.mock.calls.at(-1)![0].refresh).toBe(true);
  }
});

it("hands focus to Refresh when Try again brings the inventory in place of the failure", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await reading.answer(inventoryOf({ unavailable: { reason: "timed_out", detail: null } }));
  const retry = nextReading();
  const tryAgain = buttonNamed(dialog, "Try again")!;
  act(() => tryAgain.focus());
  press(tryAgain);
  await retry.answer(inventoryOf());
  expect(document.activeElement).toBe(buttonNamed(dialog, "Refresh connectors and MCP servers"));
});

it("shows a refused reading as a failure with Try again", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await reading.fail(CONNECTION_CLOSED());
  expect(dialog.textContent).toContain("Could not read connectors and MCP servers");
  expect(buttonNamed(dialog, "Try again")).toBeDefined();
});

it("offers Sign in or Sign out only for a server the agent signs in to with OAuth", async () => {
  const dialog = await openWithServers([
    serverOf({ name: "jira" }),
    serverOf({ name: "wiki", sign_in: "signed_in" }),
    serverOf({ name: "bearer", sign_in: "needs_sign_in", oauth_sign_in: false }),
    serverOf({ name: "notes", sign_in: "not_needed", oauth_sign_in: false }),
  ]);
  const names = [...dialog.querySelectorAll("button")].map((button) => button.getAttribute("aria-label"));
  expect(names).toContain(`Sign in to ${isolate("jira")}`);
  expect(names).toContain(`Sign out of ${isolate("wiki")}`);
  expect(names.filter((name) => /^Sign (in|out)/.test(name ?? ""))).toHaveLength(2);
});

it("offers no sign-in on a platform connector or on a Claude Code server", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await reading.answer(inventoryOf({
    connectors: [{ id: "a", name: "Gmail", state: "not_connected", plugins: [] }],
    servers: [serverOf({ oauth_sign_in: false })],
  }));
  expect(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)).toBeUndefined();
  expect([...dialog.querySelectorAll("button")].map((button) => button.textContent)).not.toContain("Sign in");
});

it("signs in, showing the wait for the browser with Cancel until it ends, then reads the inventory again", async () => {
  const dialog = await openWithServers([serverOf(), serverOf({ name: "wiki" })]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  expect(request.mock.calls.at(-1)![0]).toEqual({ type: "sign_in_account_mcp_server", agent: "codex", account: "codex-work", server: "jira" });
  const text = () => dialog.textContent!;
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
  expect(dialog.textContent).not.toContain("Waiting for the sign-in");
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
  expect(dialog.textContent).not.toContain("Waiting for the sign-in");
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
  fakeTimers();
  const dialog = await openWithServers([serverOf()]);
  const auth = nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  nextReading();
  await act(async () => vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  await auth.end({ status: "succeeded", detail: null });
  await act(async () => vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS - 1));
  expect(dialog.textContent).not.toContain("Signed in to");
  await act(async () => vi.advanceTimersByTime(1));
  expect(dialog.textContent).toContain(`Signed in to ${isolate("jira")}.`);
});

const alertRole = (dialog: HTMLElement) => dialog.querySelector("[role=alert]");

it("announces a first failure that answers right after the dialog opened a focus move's delay later, and a later one at once", async () => {
  const reading = nextReading();
  fakeTimers();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await reading.fail(CONNECTION_CLOSED());
  expect(dialog.textContent).toContain("Could not read connectors and MCP servers");
  expect(alertRole(dialog)).toBeNull();
  await act(async () => vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  expect(alertRole(dialog)).not.toBeNull();
  const retry = nextReading();
  press(buttonNamed(dialog, "Try again")!);
  await retry.answer(inventoryOf({ unavailable: { reason: "timed_out", detail: null } }));
  expect(alertRole(dialog)).not.toBeNull();
});

it("hands focus to the new Alert's Try again when one failure's Alert replaces another's", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await reading.fail(CONNECTION_CLOSED());
  const retry = nextReading();
  const tryAgain = buttonNamed(dialog, "Try again")!;
  act(() => tryAgain.focus());
  press(tryAgain);
  await retry.answer(inventoryOf({ unavailable: { reason: "timed_out", detail: null } }));
  expect(dialog.textContent).toContain("Reading timed out");
  expect(document.activeElement).toBe(buttonNamed(dialog, "Try again"));
});

it("hands focus to Refresh when an Alert with no Try again replaces the one that held it", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await reading.fail(CONNECTION_CLOSED());
  const retry = nextReading();
  const tryAgain = buttonNamed(dialog, "Try again")!;
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
  expect(document.activeElement).toBe(buttonNamed(dialog, "Try again"));
});

it("leaves focus on the page alone when a reading answers with nothing removed from under it", async () => {
  const first = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  const mountRefocusSettled = trackMountRefocus();
  await first.answer(inventoryOf());
  await mountRefocusSettled();
  const second = nextReading();
  press(buttonNamed(dialog, "Refresh connectors and MCP servers")!);
  // The dialog's focus containment would take focus back a frame after the blur, whatever the reading did.
  framesHeld();
  act(() => (document.activeElement as HTMLElement | null)?.blur());
  const before = document.activeElement;
  await second.fail(CONNECTION_CLOSED());
  expect(document.activeElement).toBe(before);
  expect(document.activeElement).not.toBe(buttonNamed(dialog, "Try again"));
});

it("shows one progress line and neither Refresh nor the age until the first answer is in", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  expect(dialog.textContent).toContain("Reading connectors and MCP servers…");
  expect(spinners(dialog)).toBe(1);
  expect(dialog.querySelector('[role="progressbar"]')).toBeNull();
  expect(dialog.textContent).not.toContain("Refreshing");
  expect(dialog.textContent).not.toContain("Updated");
  expect(buttonNamed(dialog, REFRESH)).toBeUndefined();
  await reading.answer(inventoryOf());
  expect(spinners(dialog)).toBe(0);
  expect(dialog.textContent).not.toContain("Reading connectors and MCP servers…");
  expect(buttonNamed(dialog, REFRESH)).toBeDefined();
});

it.each<[string, (reading: ReturnType<typeof nextReading>) => Promise<void>]>([
  ["an inventory", (reading) => reading.answer(inventoryOf())],
  ["an unavailable inventory", (reading) => reading.answer(inventoryOf({ unavailable: { reason: "timed_out", detail: null } }))],
  ["a refused reading", (reading) => reading.fail(CONNECTION_CLOSED())],
])("brings the Refresh button in with the first answer, %s", async (_, answer) => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  expect(buttonNamed(dialog, REFRESH)).toBeUndefined();
  await answer(reading);
  expect(buttonNamed(dialog, REFRESH)).toBeDefined();
  expect(spinners(dialog)).toBe(0);
});

it("says Refreshing in place of the age while a later reading runs, with Refresh pending and the lists kept", async () => {
  const first = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await first.answer(inventoryOf({ connectors: [{ id: "a", name: "Gmail", state: "connected", plugins: [] }] }));
  expect(dialog.textContent).toContain("Updated just now");
  expect(dialog.textContent).not.toContain("Refreshing");
  const second = nextReading();
  press(buttonNamed(dialog, REFRESH)!);
  expect(dialog.textContent).toContain("Refreshing");
  expect(dialog.textContent).not.toContain("Updated");
  expect(buttonNamed(dialog, REFRESH)!.getAttribute("aria-busy")).toBe("true");
  expect(dialog.textContent).toContain("Gmail");
  expect(dialog.textContent).not.toContain("Reading connectors and MCP servers…");
  await second.answer(inventoryOf({ connectors: [{ id: "a", name: "Gmail", state: "connected", plugins: [] }] }));
  expect(dialog.textContent).not.toContain("Refreshing");
  expect(dialog.textContent).toContain("Updated just now");
});

it("shows no age over an unavailable inventory, with Refreshing while it is read again", async () => {
  const first = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await first.answer(inventoryOf({ unavailable: { reason: "failed", detail: null } }));
  expect(dialog.textContent).not.toContain("Updated");
  expect(dialog.textContent).not.toContain("Refreshing");
  const second = nextReading();
  press(buttonNamed(dialog, REFRESH)!);
  expect(dialog.textContent).toContain("Refreshing");
  await second.answer(inventoryOf());
  expect(dialog.textContent).not.toContain("Refreshing");
  expect(dialog.textContent).toContain("Updated just now");
});

it.each<[string, string, string]>([
  ["Claude Code", "claude-work", "Still reading… Claude Code can take about a minute."],
  ["Codex", "codex-work", "Still reading… Codex can take about a minute."],
])("says a %s reading that has not answered by SLOW_READING_MS is still going, until the answer comes", async (tab, id, line) => {
  fakeTimers();
  const reading = nextReading();
  const { container } = mount(tab === "Claude Code" ? undefined : tab);
  const dialog = await openMcpOf(container, id);
  await act(async () => vi.advanceTimersByTime(SLOW_READING_MS - 1));
  expect(dialog.textContent).toContain("Reading connectors and MCP servers…");
  expect(dialog.textContent).not.toContain("Still reading");
  await act(async () => vi.advanceTimersByTime(1));
  expect(dialog.textContent).toContain(line);
  expect(dialog.textContent).not.toContain("Reading connectors and MCP servers…");
  expect(spinners(dialog)).toBe(1);
  expect(buttonNamed(dialog, REFRESH)).toBeUndefined();
  await reading.answer(inventoryOf({ agent: tab === "Claude Code" ? "claude" : "codex", account: id }));
  expect(dialog.textContent).not.toContain("Still reading");
  expect(spinners(dialog)).toBe(0);
});

it("does not say a later reading is slow, since the lists are already on show", async () => {
  fakeTimers();
  const first = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await first.answer(inventoryOf());
  nextReading();
  press(buttonNamed(dialog, REFRESH)!);
  await act(async () => vi.advanceTimersByTime(SLOW_READING_MS));
  expect(dialog.textContent).not.toContain("Still reading");
  expect(dialog.textContent).toContain("Refreshing");
});

it("shows the reading as timed out when no answer comes within READING_CEILING_MS", async () => {
  fakeTimers();
  nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await act(async () => vi.advanceTimersByTime(READING_CEILING_MS - 1));
  expect(dialog.textContent).not.toContain("Reading timed out");
  expect(spinners(dialog)).toBe(1);
  await act(async () => vi.advanceTimersByTime(1));
  expect(dialog.textContent).toContain("Reading timed out");
  expect(dialog.textContent).toContain("Claude Code did not answer in time.");
  expect(dialog.textContent).toContain("Make sure the servers it connects to are reachable, then try again.");
  expect(buttonNamed(dialog, "Try again")).toBeDefined();
  expect(buttonNamed(dialog, REFRESH)).toBeDefined();
  expect(spinners(dialog)).toBe(0);
  expect(dialog.textContent).not.toContain("Reading connectors and MCP servers…");
  expect(dialog.textContent).not.toContain("Still reading");
  expect(dialog.textContent).not.toContain("Updated");
});

it("shows a later reading that does not answer within READING_CEILING_MS as timed out in place of the lists", async () => {
  fakeTimers();
  const first = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await first.answer(inventoryOf({ connectors: [{ id: "a", name: "Gmail", state: "connected", plugins: [] }] }));
  nextReading();
  press(buttonNamed(dialog, REFRESH)!);
  await act(async () => vi.advanceTimersByTime(READING_CEILING_MS));
  expect(dialog.textContent).toContain("Reading timed out");
  expect(dialog.textContent).not.toContain("Gmail");
  expect(dialog.textContent).not.toContain("Refreshing");
  expect(buttonNamed(dialog, REFRESH)!.getAttribute("aria-busy")).toBeNull();
});

it("lets the daemon's late answer replace the timed-out alert", async () => {
  fakeTimers();
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await act(async () => vi.advanceTimersByTime(READING_CEILING_MS));
  expect(dialog.textContent).toContain("Reading timed out");
  await reading.answer(inventoryOf({ connectors: [{ id: "a", name: "Gmail", state: "connected", plugins: [] }] }));
  expect(dialog.textContent).not.toContain("Reading timed out");
  expect(dialog.textContent).toContain("Gmail");
  expect(dialog.textContent).toContain("Updated just now");
});

it("lets the daemon's late failure replace the timed-out alert", async () => {
  fakeTimers();
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await act(async () => vi.advanceTimersByTime(READING_CEILING_MS));
  await reading.fail(CONNECTION_CLOSED());
  expect(dialog.textContent).not.toContain("Reading timed out");
  expect(dialog.textContent).toContain("Could not read connectors and MCP servers");
});

it("keeps the answer of the reading that Try again started over the late answer of the one that timed out", async () => {
  fakeTimers();
  const late = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await act(async () => vi.advanceTimersByTime(READING_CEILING_MS));
  const retry = nextReading();
  press(buttonNamed(dialog, "Try again")!);
  expect(readMcpInventory.mock.calls.at(-1)![0].refresh).toBe(true);
  await late.answer(inventoryOf({ connectors: [{ id: "old", name: "Stale", state: "connected", plugins: [] }] }));
  expect(dialog.textContent).not.toContain("Stale");
  expect(dialog.textContent).toContain("Refreshing");
  await retry.answer(inventoryOf({ connectors: [{ id: "new", name: "Fresh", state: "connected", plugins: [] }] }));
  expect(dialog.textContent).toContain("Fresh");
  expect(dialog.textContent).not.toContain("Stale");
  expect(dialog.textContent).not.toContain("Reading timed out");
});

it("keeps the answer of the reading that Try again started when the stale late answer comes after it has answered", async () => {
  fakeTimers();
  const late = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await act(async () => vi.advanceTimersByTime(READING_CEILING_MS));
  const retry = nextReading();
  press(buttonNamed(dialog, "Try again")!);
  await retry.answer(inventoryOf({ connectors: [{ id: "new", name: "Fresh", state: "connected", plugins: [] }] }));
  expect(dialog.textContent).toContain("Fresh");
  expect(buttonNamed(dialog, REFRESH)!.getAttribute("aria-busy")).toBeNull();
  await late.answer(inventoryOf({ connectors: [{ id: "old", name: "Stale", state: "connected", plugins: [] }] }));
  expect(dialog.textContent).toContain("Fresh");
  expect(dialog.textContent).not.toContain("Stale");
  expect(dialog.textContent).not.toContain("Reading timed out");
});

it("does nothing when a reading's timers fall due after the dialog has closed", async () => {
  fakeTimers();
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  const logged = vi.spyOn(console, "error").mockImplementation(() => {});
  onTestFinished(() => logged.mockRestore());
  await closeDialog(dialog);
  await act(async () => vi.advanceTimersByTime(SLOW_READING_MS));
  await act(async () => vi.advanceTimersByTime(READING_CEILING_MS));
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(logged).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual([]);
  expect(readMcpInventory).toHaveBeenCalledTimes(1);
  // An answer that lands now is dropped just the same.
  await reading.answer(inventoryOf());
  expect(logged).not.toHaveBeenCalled();
});

it("says a first reading under StrictMode is still going after SLOW_READING_MS, once", async () => {
  fakeTimers();
  const reading = nextReading();
  const { container } = mount(undefined, true);
  const dialog = await openMcpOf(container, "claude-work");
  expect(readMcpInventory).toHaveBeenCalledTimes(1);
  await act(async () => vi.advanceTimersByTime(SLOW_READING_MS - 1));
  expect(dialog.textContent).not.toContain("Still reading");
  await act(async () => vi.advanceTimersByTime(1));
  expect(dialog.textContent).toContain("Still reading… Claude Code can take about a minute.");
  expect(spinners(dialog)).toBe(1);
  await reading.answer(inventoryOf());
  expect(dialog.textContent).not.toContain("Still reading");
});

it("shows a first reading under StrictMode as timed out after READING_CEILING_MS", async () => {
  fakeTimers();
  nextReading();
  const { container } = mount(undefined, true);
  const dialog = await openMcpOf(container, "claude-work");
  await act(async () => vi.advanceTimersByTime(READING_CEILING_MS - 1));
  expect(dialog.textContent).not.toContain("Reading timed out");
  await act(async () => vi.advanceTimersByTime(1));
  expect(dialog.textContent).toContain("Reading timed out");
  expect(buttonNamed(dialog, "Try again")).toBeDefined();
  expect(spinners(dialog)).toBe(0);
});

it("lays the platform connectors out as a table of Name, Status and Details, labelled by its heading", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await reading.answer(inventoryOf({
    connectors: [
      { id: "claude.ai Gmail", name: "Gmail", state: "connected", plugins: [] },
      { id: "claude.ai Slack", name: "Slack", state: "not_connected", plugins: ["chat", "ops"] },
      { id: "claude.ai Drive", name: "Drive", state: "unknown", plugins: [] },
    ],
  }));
  const table = tableOf(dialog, "Platform connectors");
  expect(table.getAttribute("role")).toBe("grid");
  expect(headersOf(table)).toEqual(["Name", "Status", "Details"]);
  expect(table.querySelectorAll('tbody [role="row"]')).toHaveLength(3);
  expect(rowOf(table, "Gmail").texts).toEqual(["Gmail", "Connected", ""]);
  expect(rowOf(table, "Slack").texts).toEqual(["Slack", "Not connected", "From plugins chat and ops"]);
  expect(rowOf(table, "Drive").texts).toEqual(["Drive", "State unknown", ""]);
  // The platform link and account note stay with the group.
  expect(dialog.querySelector("a[data-external-link]")!.textContent).toBe("Manage on claude.ai");
  expect(dialog.textContent).toContain("To connect or disconnect connectors, log in to claude.ai with the account this one uses.");
});

it("lays the local servers out as a table of Name, Sign-in, Runtime and Details, with no Actions column when none signs in with OAuth", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await reading.answer(inventoryOf({
    servers: [
      serverOf({ name: "linear", origin: "https://mcp.linear.app/mcp", sign_in: "needs_sign_in", oauth_sign_in: false }),
      serverOf({ name: "notes", origin: null, sign_in: "not_needed", runtime: "running", tool_count: 3, oauth_sign_in: false }),
    ],
  }));
  const table = tableOf(dialog, "Local MCP servers");
  expect(headersOf(table)).toEqual(["Name", "Sign-in", "Runtime", "Details"]);
  expect(rowOf(table, "notes").texts).toEqual(["notes", "Not needed", "Running", "3 tools"]);
  const linear = rowOf(table, "linear").texts;
  expect(linear.slice(0, 3)).toEqual(["linear", "Needs sign-in", "Unknown"]);
  expect(linear[3]).toContain("https://mcp.linear.app/mcp");
  expect(linear[3]).toContain("To sign in, run /mcp in a session of this account.");
  expect(table.querySelector("button")).toBeNull();
});

it.each<[LocalMcpServer["sign_in"], string]>([
  ["signed_in", "Signed in"],
  ["needs_sign_in", "Needs sign-in"],
  ["not_needed", "Not needed"],
  ["unknown", "Unknown"],
])("words a server whose sign-in is %s as %s in its Sign-in cell", async (signIn, word) => {
  const reading = nextReading();
  const { container } = mount("Codex");
  const dialog = await openMcpOf(container, "codex-work");
  await reading.answer(inventoryOf({ agent: "codex", account: "codex-work", servers: [serverOf({ sign_in: signIn, oauth_sign_in: false })] }));
  expect(rowOf(tableOf(dialog, "Local MCP servers"), "jira").texts[1]).toBe(word);
});

it.each<[LocalMcpServer["runtime"], string]>([
  ["running", "Running"],
  ["failed", "Failed"],
  ["unknown", "Unknown"],
])("words a server whose runtime is %s as %s in its Runtime cell", async (runtime, word) => {
  const reading = nextReading();
  const { container } = mount("Codex");
  const dialog = await openMcpOf(container, "codex-work");
  await reading.answer(inventoryOf({ agent: "codex", account: "codex-work", servers: [serverOf({ runtime, runtime_message: null, oauth_sign_in: false })] }));
  expect(rowOf(tableOf(dialog, "Local MCP servers"), "jira").texts[2]).toBe(word);
});

it("puts a bare mark right after a failed server's Failed badge, its popover holding only what the agent said", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await reading.answer(inventoryOf({
    servers: [
      serverOf({ name: "files", origin: null, sign_in: "not_needed", runtime: "failed", runtime_message: "spawn ENOENT", oauth_sign_in: false }),
      serverOf({ name: "quiet", origin: null, sign_in: "not_needed", runtime: "failed", runtime_message: null, oauth_sign_in: false }),
    ],
  }));
  const table = tableOf(dialog, "Local MCP servers");
  const runtime = rowOf(table, "files").cells[2];
  const mark = runtime.querySelector<HTMLButtonElement>("button")!;
  expect(mark.getAttribute("aria-label")).toBe("Failed to connect");
  expect(mark.textContent).toBe("");
  expect(mark.previousElementSibling!.textContent).toBe("Failed");
  // No mark where the agent said nothing.
  expect(rowOf(table, "quiet").cells[2].querySelector("button")).toBeNull();
  expect(document.querySelector("[data-status-mark-popover]")).toBeNull();
  press(mark);
  await act(async () => {});
  const popover = document.querySelector("[data-status-mark-popover]")!;
  expect(popover.querySelector('[role="dialog"]')!.textContent).toBe("spawn ENOENT");
});

it("puts a server's URL, plugin and tool count in its Details cell, one to a line", async () => {
  const reading = nextReading();
  const { container } = mount("Codex");
  const dialog = await openMcpOf(container, "codex-work");
  await reading.answer(inventoryOf({
    agent: "codex",
    account: "codex-work",
    servers: [serverOf({ origin: "https://jira.example.com/mcp", plugin: "atlassian", sign_in: "signed_in", runtime: "running", tool_count: 1, oauth_sign_in: false })],
  }));
  const details = rowOf(tableOf(dialog, "Local MCP servers"), "jira").cells[3];
  expect([...details.firstElementChild!.children].map((line) => line.textContent)).toEqual(["https://jira.example.com/mcp", "From plugin atlassian", "1 tool"]);
});

it("adds a visible Actions column, with the buttons in it, when a server signs in with OAuth", async () => {
  const dialog = await openWithServers([serverOf(), serverOf({ name: "wiki", sign_in: "signed_in" }), serverOf({ name: "bearer", oauth_sign_in: false })]);
  const table = tableOf(dialog, "Local MCP servers");
  expect(headersOf(table)).toEqual(["Name", "Sign-in", "Runtime", "Details", "Actions"]);
  expect(rowOf(table, "jira").cells[4].querySelector("button")!.textContent).toBe("Sign in");
  expect(rowOf(table, "wiki").cells[4].querySelector("button")!.textContent).toBe("Sign out");
  // A server with no OAuth sign-in has the cell, empty.
  const bearer = rowOf(table, "bearer");
  expect(bearer.cells).toHaveLength(5);
  expect(bearer.cells[4].querySelector("button")).toBeNull();
});

it("shows the wait for the browser in the row's Details and Cancel beside Sign in in its Actions while a sign-in waits", async () => {
  const dialog = await openWithServers([serverOf()]);
  nextAuth();
  press(buttonNamed(dialog, `Sign in to ${isolate("jira")}`)!);
  const { cells } = rowOf(tableOf(dialog, "Local MCP servers"), "jira");
  expect(cells[3].textContent).toContain("Waiting for the sign-in in your browser…");
  expect([...cells[4].querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Cancel", "Sign in"]);
});

it("names each row by the server's name and a table by its group heading", async () => {
  const reading = nextReading();
  const { container } = mount();
  const dialog = await openMcpOf(container, "claude-work");
  await reading.answer(inventoryOf({
    connectors: [{ id: "a", name: "Gmail", state: "connected", plugins: [] }],
    servers: [serverOf({ name: "linear", oauth_sign_in: false })],
  }));
  for (const heading of ["Platform connectors", "Local MCP servers"]) {
    const table = tableOf(dialog, heading);
    expect(table.getAttribute("aria-labelledby")).toBe([...dialog.querySelectorAll("h3")].find((title) => title.textContent === heading)!.id);
  }
  const row = dialog.querySelector('tbody [role="row"]')!;
  expect(row.querySelector('[role="rowheader"]')!.textContent).toBe("Gmail");
  expect(row.getAttribute("aria-labelledby")).toBe(row.querySelector('[role="rowheader"]')!.id);
});
