// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { type Event, type InspectedAccount, type InspectedAccounts, type ClientRequestBody } from "../protocol";
import { DEFAULT_ACCOUNT_NAME } from "../accounts/defaultAccount";
import { press as pressElement } from "../components/alerts/detailsTestSupport";
import { setLanguage } from "../i18n/language";
import { ImportConfigurationDialog } from "./ImportConfigurationDialog";
import type { Inspection } from "./importPlan";
import { agentSettingLinesOf, cardTitled, mountDialog, openTab, press, settingLinesOf } from "./testSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const account = (fields: Partial<InspectedAccount> & Pick<InspectedAccount, "name">): InspectedAccount => ({
  directory: `~/.${fields.name.toLowerCase()}`,
  foreign_directory: false,
  proposed_directory: `/Users/dev/.${fields.name.toLowerCase()}`,
  status: "new",
  in_use_by: null,
  error: null,
  disabled: false,
  ...fields,
});

const ACCOUNTS: InspectedAccounts = {
  agents_to_add: ["codex"],
  agents: [
    {
      agent: "claude",
      default_name: "Main",
      default_name_applies: true,
      default_disabled: null,
      default_disabled_applies: false,
      accounts: [
        account({ name: "Work" }),
        account({ name: "Home", status: "existing" }),
        account({ name: "Team", status: "directory_in_use", in_use_by: "Shared", disabled: true }),
        account({ name: "Odd", status: "invalid", error: { code: "path_not_absolute", params: { path: "rel", tilde: "~" }, message: "m" } }),
      ],
    },
    { agent: "codex", default_name: null, default_name_applies: false, default_disabled: true, default_disabled_applies: true, accounts: [] },
  ],
  agent_settings: {
    auto_select_account: true,
    current_auto_select_account: false,
    auto_switch_on_usage_limit: false,
    current_auto_switch_on_usage_limit: false,
    auto_start_quota_windows: false,
    current_auto_start_quota_windows: true,
    provider_settings: {},
    current_provider_settings: {},
    agent_tags: { codex: ["review"] },
    current_agent_tags: {},
  },
};

const DOCUMENT = { format: "octoboard-configuration", version: 1 };
const project = {
  name: "Site",
  path: "/a/site",
  foreign_path: false,
  remote_url: null,
  offers: { exported_path: "/a/site", default_clone_dir: null, clone: null },
  default: "exported_path" as const,
  in_document_clone_dir: null,
};
const CONSOLES: Inspection["consoles"] = [
  {
    name: "Laptop",
    account_fallbacks: [{ agent: "claude", name: "Work" }],
    projects: [{ ...project, account_fallbacks: [{ agent: "codex", name: "Gone" }] }],
  },
];

const IMPORTED: Event = { type: "configuration_imported", id: null, consoles: [], settings_applied: null, settings_error: null, accounts: null };

function mount(inspection: Omit<Inspection, "type">, sent?: (body: ClientRequestBody) => Event) {
  const mounted = mountDialog(
    (onClose) => <ImportConfigurationDialog onClose={onClose} />,
    (body) => Promise.resolve(body.type === "inspect_configuration" ? { type: "configuration_inspected", ...inspection } : (sent?.(body) ?? IMPORTED)),
    { save: vi.fn(), open: vi.fn(async () => ({ name: "mine.json", text: JSON.stringify(DOCUMENT) })) },
  );
  return mounted;
}

async function reviewing(inspection: Omit<Inspection, "type"> = { accounts: ACCOUNTS, consoles: CONSOLES, id: null, settings: null, settings_sections: [] }) {
  const mounted = mount(inspection);
  await press("Choose file");
  return mounted;
}

const tabs = () => Array.from(document.body.querySelectorAll("[role=tab]")).map((tab) => tab.getAttribute("aria-label") ?? tab.textContent);
const row = (name: string) => Array.from(document.body.querySelectorAll("li")).find((li) => li.querySelector("span.font-medium")?.textContent === name)!;
const agentBox = (name: string) => Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=checkbox]")).find((input) => input.labels?.[0]?.textContent === name)!;
const accountsBox = () => Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=checkbox]")).find((input) => input.labels?.[0]?.textContent === "Agent accounts")!;

/** The popover of the warning mark in `scope`, opened the way a press from assistive technology does: its text, and
 * the mark's accessible name. The popover is closed again. */
async function markPopover(scope: Element): Promise<{ name: string; text: string }> {
  const mark = scope.querySelector<HTMLElement>("button[data-status-mark]")!;
  const name = mark.getAttribute("aria-label")!;
  pressElement(mark);
  const dialog = document.body.querySelector(`[role="dialog"][aria-label="${name}"]`)!;
  const text = dialog.textContent ?? "";
  await act(async () => void dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  return { name, text };
}

async function choose(name: string, option: string): Promise<void> {
  await act(async () => row(name).querySelector<HTMLButtonElement>("button[aria-haspopup=listbox]")!.click());
  const item = Array.from(document.body.querySelectorAll<HTMLElement>("[role=option]")).find((o) => o.textContent === option);
  await act(async () => item!.click());
}

function type(input: HTMLInputElement, value: string): void {
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

it("has a tab for each region the document carries, and the accounts tab says it needs attention", async () => {
  await reviewing({ accounts: ACCOUNTS, consoles: CONSOLES, settings: null, settings_sections: [], id: null });
  expect(tabs()).toEqual(["Consoles and projects, needs attention", "Agent accounts, needs attention"]);
});

it("says why a tab needs attention in a tooltip when it takes keyboard focus", async () => {
  await reviewing();
  const tab = Array.from(document.body.querySelectorAll<HTMLElement>("[role=tab]")).find((t) => t.textContent === "Agent accounts")!;
  act(() => setInteractionModality("keyboard"));
  act(() => tab.focus());
  expect(document.querySelector("[role=tooltip]")?.textContent).toBe("This part has something that needs your attention.");
  act(() => tab.blur());
  expect(document.querySelector("[role=tooltip]")).toBeNull();
});

it("says each console is created as a new one at the top of the consoles panel, and the intro only names the file", async () => {
  await reviewing();
  expect(document.body.textContent).toContain("These are the contents of mine.json.");
  expect(document.body.textContent).toContain("Each console is created as a new one; your existing consoles stay as they are.");
  await openTab("Agent accounts");
  expect(document.body.textContent).not.toContain("Each console is created as a new one");
  expect(document.body.textContent).toContain("These are the contents of mine.json.");
});

it("keeps the tab bar when the document carries only one region", async () => {
  await reviewing({ accounts: ACCOUNTS, consoles: [], id: null, settings: null, settings_sections: [] });
  expect(tabs()).toEqual(["Agent accounts, needs attention"]);
  // Importing that file is importing that region: nothing to check but the agents.
  expect(accountsBox()).toBeUndefined();
  expect(document.body.textContent).toContain("The default account will be named Main.");
});

it("shows each account with its status, whether it is disabled and the directory the file gives it, and never sharing", async () => {
  await reviewing();
  await openTab("Agent accounts");
  expect(accountsBox().checked).toBe(true);
  expect(row("Work").textContent).toContain("New");
  expect(row("Work").textContent).not.toContain("Shares");
  expect(row("Work").querySelector('[role=img][aria-label="Directory in the file"]')).not.toBeNull();
  expect(row("Work").textContent).not.toContain("Directory in the file");
  expect(row("Work").textContent).toContain("~/.work");
  // An account used as it is has no directory line and no choice.
  expect(row("Home").textContent).toContain("Already exists, used as it is");
  expect(row("Home").querySelector('[role=img][aria-label="Directory in the file"]')).toBeNull();
  expect(row("Home").querySelector("button[aria-haspopup=listbox]")).toBeNull();
  expect(row("Team").textContent).toContain("Disabled");
  expect(row("Team").textContent).toContain("~/.team");
  // An agent to add says so in its own card.
  expect(cardTitled(document.body, "Codex")!.textContent).toContain("Will be added to the agent list.");
  expect(cardTitled(document.body, "Claude Code")!.textContent).not.toContain("added to the agent list");
  expect(document.body.textContent).toContain("The default account will be named Main.");
  expect(document.body.textContent).toContain("The accounts are created without being logged in.");
});

it("tells that the accounts are not logged in only while an imported agent has an account to create", async () => {
  const note = "The accounts are created without being logged in.";
  await reviewing();
  await openTab("Agent accounts");
  expect(document.body.textContent).toContain(note);
  // Work is the only account that is created: the others exist already or start as left out.
  await choose("Work", "Leave out");
  expect(document.body.textContent).not.toContain(note);
  await choose("Odd", "Another directory");
  expect(document.body.textContent).toContain(note);
  // Nor does an agent that is unchecked.
  await act(async () => agentBox("Claude Code").click());
  expect(document.body.textContent).not.toContain(note);
});

it("shows the switches of the file in the first card, and nothing of any one agent in it", async () => {
  await reviewing();
  await openTab("Agent accounts");
  const card = cardTitled(document.body, "Agent account settings")!;
  expect(card.textContent).not.toContain("added to the agent list");
  expect(card.textContent).not.toContain("default account");
  expect(settingLinesOf(card.querySelector("ul:not([role])")!)).toEqual({
    "Automatically select an account": { badge: "Turns on", note: "A new session then uses the account whose weekly limit resets soonest." },
    "Automatically switch accounts at the usage limit": { badge: "No change" },
    "Start usage windows automatically": { badge: "Turns off" },
  });
  const headings = Array.from(document.body.querySelectorAll("section h3"), (heading) => heading.textContent);
  expect(headings.slice(-3)).toEqual(["Agent account settings", "Claude Code", "Codex"]);
});

it("puts an agent's default account changes, then its settings, then its accounts under a subheading, in the agent's own card", async () => {
  await reviewing();
  await openTab("Agent accounts");
  const claude = cardTitled(document.body, "Claude Code")!;
  expect(claude.textContent).toContain("The default account will be named Main.");
  expect(claude.textContent).not.toContain("will be disabled");
  expect(agentSettingLinesOf(claude)).toEqual({});
  const order = Array.from(claude.children, (child) => child.tagName + ":" + (child.textContent ?? "").slice(0, 20));
  expect(order[1]).toMatch(/^DIV:The default account/);
  expect(order.some((entry) => entry.startsWith("H4:Accounts"))).toBe(true);
  expect(claude.querySelectorAll("h4")).toHaveLength(1);
  // The subheading is a filled band of its own: no groove at its edges, nor on the block above it.
  const subheading = claude.querySelector("h4")!;
  expect(subheading.className).toContain("bg-(--control-fill)");
  expect(subheading.className).not.toContain("groove");
  expect(subheading.previousElementSibling!.className).not.toContain("groove-b");
  expect(subheading.nextElementSibling?.tagName).toBe("UL");
});

it("gives an agent with no account of its own a card of its default account's changes and its tags, without the accounts part", async () => {
  await reviewing();
  await openTab("Agent accounts");
  const codex = cardTitled(document.body, "Codex")!;
  expect(codex.textContent).toContain("Will be added to the agent list.");
  expect(codex.textContent).toContain("The default account will be disabled.");
  expect(agentSettingLinesOf(codex)).toEqual({ Tags: { badge: "Set to the file's", values: ["review (now No tags)"] } });
  expect(codex.querySelector("h4")).toBeNull();
  // The header, then the one stack of blocks (the default-account lines and the settings); no accounts list.
  expect(Array.from(codex.children).map((child) => child.tagName)).toEqual(["DIV", "DIV"]);
});

it("draws no card for an agent the file gives nothing", async () => {
  const [claude] = ACCOUNTS.agents;
  const idle = { agent: "codex" as const, default_name: null, default_name_applies: false, default_disabled: null, default_disabled_applies: false, accounts: [] };
  await reviewing({ accounts: { ...ACCOUNTS, agents_to_add: [], agent_settings: { ...ACCOUNTS.agent_settings!, agent_tags: {} }, agents: [claude, idle] }, consoles: [], id: null, settings: null, settings_sections: [] });
  const headings = Array.from(document.body.querySelectorAll("section h3"), (heading) => heading.textContent);
  expect(headings).toContain("Claude Code");
  expect(headings).not.toContain("Codex");
});

it("imports every agent to start with, and sends the agents checked", async () => {
  const mounted = await reviewing();
  await openTab("Agent accounts");
  expect(agentBox("Claude Code").checked).toBe(true);
  expect(agentBox("Codex").checked).toBe(true);
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_accounts: true, apply_agents: ["claude", "codex"] });
});

it("collapses an agent that is unchecked to its heading, left out, and imports nothing of it", async () => {
  const mounted = await reviewing();
  await openTab("Agent accounts");
  await choose("Work", "Leave out");
  await act(async () => agentBox("Claude Code").click());
  const claude = cardTitled(document.body, "Claude Code")!;
  expect(claude.textContent).toBe("Claude CodeLeft out");
  expect(row("Work")).toBeUndefined();
  expect(cardTitled(document.body, "Codex")!.textContent).toContain("Will be added to the agent list.");
  await press("Import");
  const request = mounted.sent().at(-1)!;
  expect(request).toMatchObject({ apply_agents: ["codex"], account_resolutions: [] });
});

it("keeps the choices made for an agent's accounts when it is unchecked and checked again", async () => {
  await reviewing();
  await openTab("Agent accounts");
  await choose("Work", "Leave out");
  await act(async () => agentBox("Claude Code").click());
  await act(async () => agentBox("Claude Code").click());
  expect(row("Work").querySelector("button[aria-haspopup=listbox]")!.textContent).toContain("Leave out");
});

it("keeps the agents unchecked while the accounts are unchecked and checked again", async () => {
  await reviewing();
  await openTab("Agent accounts");
  await act(async () => agentBox("Codex").click());
  await act(async () => accountsBox().click());
  await act(async () => accountsBox().click());
  await openTab("Agent accounts");
  expect(agentBox("Codex").checked).toBe(false);
  expect(agentBox("Claude Code").checked).toBe(true);
});

it("does not stop the import for a directory typed for an account of an agent that is unchecked", async () => {
  const mounted = await reviewing();
  await openTab("Agent accounts");
  await choose("Odd", "Another directory");
  await act(async () => agentBox("Claude Code").click());
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_agents: ["codex"] });
});

it("counts a reference to an account of an agent that is unchecked as a fallback, like one to an account left out", async () => {
  await reviewing();
  const marks = () => Array.from(document.body.querySelectorAll("button[data-status-mark]")).map((mark) => mark.getAttribute("aria-label"));
  expect(marks()).toEqual(["Accounts not set up for ⁨Site⁩"]);
  await openTab("Agent accounts");
  await act(async () => agentBox("Claude Code").click());
  await openTab("Consoles and projects");
  expect(marks()).toEqual(["Accounts not set up for ⁨Laptop⁩", "Accounts not set up for ⁨Site⁩"]);
});

it("says why an account cannot be created in a warning mark after its name, not as text in its row", async () => {
  await reviewing();
  await openTab("Agent accounts");
  expect(row("Team").textContent).not.toContain("in use");
  expect(row("Odd").textContent).not.toContain("Cannot be created");
  expect(row("Work").querySelector("button[data-status-mark]")).toBeNull();
  const inUse = await markPopover(row("Team"));
  expect(inUse.name).toBe("Directory of ⁨Team⁩ is in use");
  expect(inUse.text).toContain("The directory is already in use by Shared.");
  expect(inUse.text).toContain("The account is left out unless you choose another directory for it.");
  const invalid = await markPopover(row("Odd"));
  expect(invalid.name).toBe("⁨Odd⁩ cannot be created");
  expect(invalid.text).toContain("This account cannot be created: ");
  expect(invalid.text).toContain("rel is not an absolute path");
});

it("tells an account whose directory is from another operating system so, and never that it is not absolute", async () => {
  const foreign = account({
    name: "Win",
    directory: "C:\\Users\\ana\\.claude-win",
    foreign_directory: true,
    proposed_directory: null,
    status: "invalid",
    error: { code: "config_dir_not_absolute", params: { agent: "claude" }, message: "m" },
  });
  const named = account({ name: "Dup", foreign_directory: true, proposed_directory: null, status: "invalid", error: NAME_TAKEN });
  await reviewing({ accounts: { ...ACCOUNTS, agents: [{ ...ACCOUNTS.agents[0], accounts: [foreign, named] }] }, consoles: [], id: null, settings: null, settings_sections: [] });
  const notice = "The directory in the file was written on another operating system, so it cannot be used here.";
  // In the account's mark, never a loose line in the row.
  expect(row("Win").textContent).not.toContain(notice);
  const { text } = await markPopover(row("Win"));
  expect(text).toContain(notice);
  expect(text).toContain("The account is left out unless you choose another directory for it.");
  expect(text).not.toContain("absolute");
  expect((await markPopover(row("Dup"))).text).not.toContain(notice);
  expect(row("Win").querySelector("button[aria-haspopup=listbox]")!.textContent).toBe("Leave out");
  await choose("Win", "Another directory");
  expect((await markPopover(row("Win"))).text).toContain(notice);
});

it("words the default account that holds a directory in the current language", async () => {
  onTestFinished(() => setLanguage("en"));
  const [claude] = ACCOUNTS.agents;
  const inUse = account({ name: "Team", status: "directory_in_use", in_use_by: DEFAULT_ACCOUNT_NAME });
  await reviewing({ accounts: { ...ACCOUNTS, agents: [{ ...claude, accounts: [inUse] }] }, consoles: [], id: null, settings: null, settings_sections: [] });
  await act(async () => setLanguage("zh-Hans"));
  const { text } = await markPopover(row("Team"));
  expect(text).toContain("该目录已被 默认 使用。");
  expect(text).not.toContain(DEFAULT_ACCOUNT_NAME);
});

const NAME_TAKEN = { code: "account_name_taken", params: { agent: "Codex", name: "Dup", existing: "Main" }, message: "taken" };

it("offers only Leave out for an account whose name is the problem, and words the mark without promising a fix", async () => {
  const codex = ACCOUNTS.agents[1];
  const dup = account({ name: "Dup", status: "invalid", error: NAME_TAKEN });
  const odd = account({ name: "Bad", status: "invalid", error: { code: "path_not_absolute", params: { path: "rel", tilde: "~" }, message: "m" } });
  await reviewing({ accounts: { ...ACCOUNTS, agents: [ACCOUNTS.agents[0], { ...codex, accounts: [dup, odd] }] }, consoles: [], id: null, settings: null, settings_sections: [] });
  await act(async () => row("Dup").querySelector<HTMLButtonElement>("button[aria-haspopup=listbox]")!.click());
  expect(Array.from(document.body.querySelectorAll("[role=option]")).map((o) => o.textContent)).toEqual(["Leave out"]);
  await act(async () => document.body.querySelector<HTMLElement>("[role=option]")!.click());
  expect(row("Dup").querySelector("input")).toBeNull();
  const { text } = await markPopover(row("Dup"));
  expect(text).toContain("The account is left out.");
  expect(text).not.toContain("another directory");
  // A directory that cannot be used keeps the way out, and its field's example follows the agent.
  await choose("Bad", "Another directory");
  expect(row("Bad").querySelector("input")!.getAttribute("placeholder")).toBe("Ex. ~/.codex-work");
});

it("keeps the fallback mark of a reference to an account that cannot be created for its name", async () => {
  const claude = { ...ACCOUNTS.agents[0], accounts: [account({ name: "Work", status: "invalid", error: NAME_TAKEN })] };
  await reviewing({ accounts: { ...ACCOUNTS, agents: [claude] }, consoles: CONSOLES, id: null, settings: null, settings_sections: [] });
  const marks = () => Array.from(document.body.querySelectorAll("button[data-status-mark]")).map((mark) => mark.getAttribute("aria-label"));
  expect(marks()).toEqual(["Accounts not set up for ⁨Laptop⁩", "Accounts not set up for ⁨Site⁩"]);
});

it("hides the file's directory once another directory is chosen for the account", async () => {
  await reviewing();
  await openTab("Agent accounts");
  expect(row("Odd").querySelector('[role=img][aria-label="Directory in the file"]')).not.toBeNull();
  await choose("Odd", "Another directory");
  expect(row("Odd").querySelector('[role=img][aria-label="Directory in the file"]')).toBeNull();
  expect(row("Odd").querySelector("input")!.getAttribute("placeholder")).toBe("Ex. ~/.claude-work");
});

it("leaves out an account that cannot be created as proposed until another directory is chosen, and sends only the resolutions", async () => {
  const mounted = await reviewing();
  await openTab("Agent accounts");
  expect(row("Team").querySelector("button[aria-haspopup=listbox]")!.textContent).toContain("Leave out");
  await choose("Work", "Leave out");
  await choose("Odd", "Another directory");
  type(row("Odd").querySelector("input")!, "~/odd");
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({
    type: "import_configuration",
    apply_accounts: true,
    account_resolutions: [
      { agent: "claude", account: 0, leave_out: true },
      { agent: "claude", account: 2, leave_out: true },
      { agent: "claude", account: 3, directory: "~/odd" },
    ],
  });
});

it("sends no accounts when the box is unchecked and drops their tab, and then a reference to an account the import would create is a fallback", async () => {
  const mounted = await reviewing();
  const consolesMarks = () => Array.from(document.body.querySelectorAll("button[data-status-mark]")).map((mark) => mark.getAttribute("aria-label"));
  expect(consolesMarks()).toEqual(["Accounts not set up for ⁨Site⁩"]);
  await act(async () => accountsBox().click());
  expect(tabs()).toEqual(["Consoles and projects, needs attention"]);
  expect(document.body.textContent).not.toContain("The accounts are created without being logged in.");
  expect(consolesMarks()).toEqual(["Accounts not set up for ⁨Laptop⁩", "Accounts not set up for ⁨Site⁩"]);
  await press("Import");
  const request = mounted.sent().at(-1)!;
  expect(request).not.toHaveProperty("apply_accounts");
  expect(request).not.toHaveProperty("account_resolutions");
});

it("counts a reference to an account the import would create as a fallback again once that account is left out", async () => {
  await reviewing();
  const consolesMarks = () => Array.from(document.body.querySelectorAll("button[data-status-mark]")).map((mark) => mark.getAttribute("aria-label"));
  expect(consolesMarks()).toEqual(["Accounts not set up for ⁨Site⁩"]);
  await openTab("Agent accounts");
  await choose("Work", "Leave out");
  await openTab("Consoles and projects");
  expect(consolesMarks()).toEqual(["Accounts not set up for ⁨Laptop⁩", "Accounts not set up for ⁨Site⁩"]);
  await openTab("Agent accounts");
  await choose("Work", "Create it");
  await openTab("Consoles and projects");
  expect(consolesMarks()).toEqual(["Accounts not set up for ⁨Site⁩"]);
});

it("stops at a typed account directory that is not a path, brings the accounts tab forward and marks the field", async () => {
  const mounted = await reviewing();
  await openTab("Agent accounts");
  await choose("Odd", "Another directory");
  await openTab("Consoles and projects");
  await press("Import");
  expect(mounted.sent().map((body) => body.type)).toEqual(["inspect_configuration"]);
  expect(row("Odd")).toBeDefined();
  expect(row("Odd").textContent).toContain("Enter a directory, or leave the account out.");
  expect(document.querySelector("[role=tab][aria-selected=true]")!.textContent).toContain("Agent accounts");
  type(row("Odd").querySelector("input")!, "relative");
  expect(row("Odd").textContent).toContain("Enter an absolute path");
});

it("takes no account directory into account while the accounts are not imported", async () => {
  const mounted = await reviewing();
  await openTab("Agent accounts");
  await choose("Odd", "Another directory");
  await act(async () => accountsBox().click());
  await press("Import");
  expect(mounted.sent().at(-1)!.type).toBe("import_configuration");
});

it("tells of each account's outcome in the result, with a note that the new ones are not logged in and no button for it", async () => {
  mountDialog(
    (onClose) => <ImportConfigurationDialog onClose={onClose} />,
    (body) =>
      Promise.resolve(
        body.type === "inspect_configuration"
          ? { type: "configuration_inspected", id: null, accounts: ACCOUNTS, consoles: [], settings: null, settings_sections: [] }
          : ({
            type: "configuration_imported",
            id: null,
            consoles: [],
            accounts: {
              agents_added: ["codex"],
              agents: [
                {
                  agent: "claude",
                  default_name_applied: true,
                  default_disabled_applied: true,
                  accounts: [
                    { name: "Work", status: "created", account: "a1", error: null },
                    { name: "Home", status: "existing", account: "a2", error: null },
                    { name: "Team", status: "left_out", account: null, error: null },
                    { name: "Odd", status: "failed", account: null, error: { code: "path_not_absolute", params: { path: "rel", tilde: "~" }, message: "m" } },
                  ],
                },
              ],
              agent_settings_applied: { auto_select_account: true, auto_switch_on_usage_limit: false, auto_start_quota_windows: false, provider_settings: {}, agent_tags: {} },
              agent_settings_error: null,
            }, settings_applied: null, settings_error: null,
          } as Event),
      ),
    { save: vi.fn(), open: vi.fn(async () => ({ name: "mine.json", text: JSON.stringify(DOCUMENT) })) },
  );
  await press("Choose file");
  await press("Import");
  const text = document.body.textContent ?? "";
  expect(Array.from(document.body.querySelectorAll("ul[aria-label='Totals for accounts'] > li")).map((li) => li.textContent!.replace(/·$/, ""))).toEqual(["1 account created", "1 account already existed", "1 account left out", "1 failed"]);
  const card = Array.from(document.body.querySelectorAll("section")).find((section) => document.getElementById(section.getAttribute("aria-labelledby")!)?.textContent === "Claude Code")!;
  expect(Array.from(card.querySelectorAll("li")).map((li) => li.querySelector("[dir=auto]")!.textContent)).toEqual(["Work", "Home", "Team", "Odd"]);
  const rowText = (name: string) => Array.from(card.querySelectorAll("li")).find((li) => li.textContent!.startsWith(name))!.textContent!;
  expect(rowText("Work")).toContain("Created");
  expect(rowText("Home")).toContain("Already existed");
  expect(rowText("Team")).toContain("Left out");
  expect(rowText("Odd")).toContain("Failed");
  // The reason of a failure is written in its row, not behind a mark.
  expect(rowText("Odd")).toContain("rel");
  expect(card.querySelector("button[data-status-mark]")).toBeNull();
  const codex = Array.from(document.body.querySelectorAll("section")).find((section) => document.getElementById(section.getAttribute("aria-labelledby")!)?.textContent === "Codex")!;
  expect(codex.textContent).toContain("Added to the agent list.");
  expect(card.textContent).not.toContain("added to the agent list");
  // The default account's lines stand in the agent's own card, above its accounts.
  expect(card.textContent).toContain("The default account is now named as in the file.");
  expect(card.textContent).toContain("The default account is now disabled.");
  expect(text).toContain("Agent account settings");
  expect(text).toContain("The new accounts are not logged in yet.");
  expect(Array.from(document.body.querySelectorAll("button")).map((button) => button.textContent)).not.toContain("Log in");
});
