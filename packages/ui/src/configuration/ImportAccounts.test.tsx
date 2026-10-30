// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { DEFAULT_ACCOUNT_NAME, type Event, type InspectedAccount, type InspectedAccounts, type RequestBody } from "../protocol";
import { press as pressElement } from "../components/detailsTestSupport";
import { setLanguage } from "../i18n/language";
import { ImportConfigurationDialog } from "./ImportConfigurationDialog";
import type { Inspection } from "./importPlan";
import { mountDialog, openTab, press } from "./testSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const account = (fields: Partial<InspectedAccount> & Pick<InspectedAccount, "name">): InspectedAccount => ({
  directory: `~/.${fields.name.toLowerCase()}`,
  proposed_directory: `/Users/dev/.${fields.name.toLowerCase()}`,
  status: "new",
  in_use_by: null,
  error: null,
  share_default_config: false,
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
      accounts: [
        account({ name: "Work", share_default_config: true }),
        account({ name: "Home", status: "existing" }),
        account({ name: "Team", status: "directory_in_use", in_use_by: "Shared", disabled: true }),
        account({ name: "Odd", status: "invalid", error: { code: "path_not_absolute", params: { path: "rel", tilde: "~" }, message: "m" } }),
      ],
    },
    { agent: "codex", default_name: null, default_name_applies: false, accounts: [] },
  ],
};

const DOCUMENT = { format: "octoboard-configuration", version: 1 };
const project = {
  name: "Site",
  path: "/a/site",
  remote_url: null,
  offers: { exported_path: "/a/site", default_clone_dir: null, clone: null },
  default: "exported_path" as const,
};
const CONSOLES: Inspection["consoles"] = [
  {
    name: "Laptop",
    account_fallbacks: [{ agent: "claude", name: "Work" }],
    projects: [{ ...project, account_fallbacks: [{ agent: "codex", name: "Gone" }] }],
  },
];

const IMPORTED: Event = { type: "configuration_imported", id: null, consoles: [], settings_applied: null, settings_error: null, accounts: null };

function mount(inspection: Omit<Inspection, "type">, sent?: (body: RequestBody) => Event) {
  const mounted = mountDialog(
    (onClose) => <ImportConfigurationDialog onClose={onClose} />,
    (body) => Promise.resolve(body.type === "inspect_configuration" ? { type: "configuration_inspected", ...inspection } : (sent?.(body) ?? IMPORTED)),
    { save: vi.fn(), open: vi.fn(async () => ({ name: "mine.json", text: JSON.stringify(DOCUMENT) })) },
  );
  return mounted;
}

async function reviewing(inspection: Omit<Inspection, "type"> = { accounts: ACCOUNTS, consoles: CONSOLES, id: null, settings: null }) {
  const mounted = mount(inspection);
  await press("Choose file");
  return mounted;
}

const tabs = () => Array.from(document.body.querySelectorAll("[role=tab]")).map((tab) => tab.getAttribute("aria-label") ?? tab.textContent);
const row = (name: string) => Array.from(document.body.querySelectorAll("li")).find((li) => li.querySelector("span.font-medium")?.textContent === name)!;
const accountsBox = () => Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=checkbox]")).find((input) => input.labels?.[0]?.textContent === "Accounts")!;

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
  await reviewing({ accounts: ACCOUNTS, consoles: CONSOLES, settings: null, id: null });
  expect(tabs()).toEqual(["Consoles and projects, needs attention", "Accounts, needs attention"]);
});

it("says why a tab needs attention in a tooltip when it takes keyboard focus", async () => {
  await reviewing();
  const tab = Array.from(document.body.querySelectorAll<HTMLElement>("[role=tab]")).find((t) => t.textContent === "Accounts")!;
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
  await openTab("Accounts");
  expect(document.body.textContent).not.toContain("Each console is created as a new one");
  expect(document.body.textContent).toContain("These are the contents of mine.json.");
});

it("has no tab bar when the document carries only one region", async () => {
  await reviewing({ accounts: ACCOUNTS, consoles: [], id: null, settings: null });
  expect(document.body.querySelector("[role=tab]")).toBeNull();
  // Importing that file is importing that region: nothing to check.
  expect(document.body.querySelector("input[type=checkbox]")).toBeNull();
  expect(document.body.textContent).toContain("The default account of Claude Code will be named Main.");
});

it("shows each account with its status, what it shares and the directory the file gives it", async () => {
  await reviewing();
  await openTab("Accounts");
  expect(accountsBox().checked).toBe(true);
  expect(row("Work").textContent).toContain("New");
  expect(row("Work").textContent).toContain("Shares default configuration");
  expect(row("Work").textContent).toContain("Directory in the file");
  expect(row("Work").textContent).toContain("~/.work");
  // An account used as it is has no directory line and no choice.
  expect(row("Home").textContent).toContain("Already exists, used as it is");
  expect(row("Home").textContent).not.toContain("Directory in the file");
  expect(row("Home").querySelector("button[aria-haspopup=listbox]")).toBeNull();
  expect(row("Team").textContent).toContain("Disabled");
  expect(row("Team").textContent).toContain("~/.team");
  expect(document.body.textContent).toContain("The agent Codex will be added to your agents.");
  expect(document.body.textContent).toContain("The default account of Claude Code will be named Main.");
  expect(document.body.textContent).toContain("The accounts are created without being logged in.");
});

it("says why an account cannot be created in a warning mark after its name, not as text in its row", async () => {
  await reviewing();
  await openTab("Accounts");
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

it("words the default account that holds a directory in the current language", async () => {
  onTestFinished(() => setLanguage("en"));
  const [claude] = ACCOUNTS.agents;
  const inUse = account({ name: "Team", status: "directory_in_use", in_use_by: DEFAULT_ACCOUNT_NAME });
  await reviewing({ accounts: { ...ACCOUNTS, agents: [{ ...claude, accounts: [inUse] }] }, consoles: [], id: null, settings: null });
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
  await reviewing({ accounts: { ...ACCOUNTS, agents: [ACCOUNTS.agents[0], { ...codex, accounts: [dup, odd] }] }, consoles: [], id: null, settings: null });
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
  await reviewing({ accounts: { ...ACCOUNTS, agents: [claude] }, consoles: CONSOLES, id: null, settings: null });
  const marks = () => Array.from(document.body.querySelectorAll("button[data-status-mark]")).map((mark) => mark.getAttribute("aria-label"));
  expect(marks()).toEqual(["Accounts not set up for ⁨Laptop⁩", "Accounts not set up for ⁨Site⁩"]);
});

it("hides the file's directory once another directory is chosen for the account", async () => {
  await reviewing();
  await openTab("Accounts");
  expect(row("Odd").textContent).toContain("Directory in the file");
  await choose("Odd", "Another directory");
  expect(row("Odd").textContent).not.toContain("Directory in the file");
  expect(row("Odd").querySelector("input")!.getAttribute("placeholder")).toBe("Ex. ~/.claude-work");
});

it("leaves out an account that cannot be created as proposed until another directory is chosen, and sends only the resolutions", async () => {
  const mounted = await reviewing();
  await openTab("Accounts");
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
  expect(tabs()).toEqual([]);
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
  await openTab("Accounts");
  await choose("Work", "Leave out");
  await openTab("Consoles and projects");
  expect(consolesMarks()).toEqual(["Accounts not set up for ⁨Laptop⁩", "Accounts not set up for ⁨Site⁩"]);
  await openTab("Accounts");
  await choose("Work", "Create it");
  await openTab("Consoles and projects");
  expect(consolesMarks()).toEqual(["Accounts not set up for ⁨Site⁩"]);
});

it("stops at a typed account directory that is not a path, brings the accounts tab forward and marks the field", async () => {
  const mounted = await reviewing();
  await openTab("Accounts");
  await choose("Odd", "Another directory");
  await openTab("Consoles and projects");
  await press("Import");
  expect(mounted.sent().map((body) => body.type)).toEqual(["inspect_configuration"]);
  expect(row("Odd")).toBeDefined();
  expect(row("Odd").textContent).toContain("Enter a directory, or leave the account out.");
  expect(document.querySelector("[role=tab][aria-selected=true]")!.textContent).toContain("Accounts");
  type(row("Odd").querySelector("input")!, "relative");
  expect(row("Odd").textContent).toContain("Enter an absolute path");
});

it("takes no account directory into account while the accounts are not imported", async () => {
  const mounted = await reviewing();
  await openTab("Accounts");
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
          ? { type: "configuration_inspected", id: null, accounts: ACCOUNTS, consoles: [], settings: null }
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
                  accounts: [
                    { name: "Work", status: "created", account: "a1", error: null, sharing_warning: { code: "account_sharing_failed", params: {}, message: "no" } },
                    { name: "Home", status: "existing", account: "a2", error: null, sharing_warning: null },
                    { name: "Team", status: "left_out", account: null, error: null, sharing_warning: null },
                    { name: "Odd", status: "failed", account: null, error: { code: "path_not_absolute", params: { path: "rel", tilde: "~" }, message: "m" }, sharing_warning: null },
                  ],
                },
              ],
            }, settings_applied: null, settings_error: null,
          } as Event),
      ),
    { save: vi.fn(), open: vi.fn(async () => ({ name: "mine.json", text: JSON.stringify(DOCUMENT) })) },
  );
  await press("Choose file");
  await press("Import");
  const text = document.body.textContent ?? "";
  expect(Array.from(document.body.querySelectorAll("ul[aria-label='Totals for accounts'] > li")).map((li) => li.textContent!.replace(/·$/, ""))).toEqual(["1 account created", "1 account already existed", "1 account left out", "1 failed"]);
  const card = Array.from(document.body.querySelectorAll("section")).find((section) => section.querySelector(":scope > div > div > h3")?.textContent === "Claude Code")!;
  expect(Array.from(card.querySelectorAll("li")).map((li) => li.querySelector("[dir=auto]")!.textContent)).toEqual(["Work", "Home", "Team", "Odd"]);
  const rowText = (name: string) => Array.from(card.querySelectorAll("li")).find((li) => li.textContent!.startsWith(name))!.textContent!;
  expect(rowText("Work")).toContain("Created");
  expect(rowText("Home")).toContain("Already existed");
  expect(rowText("Team")).toContain("Left out");
  expect(rowText("Odd")).toContain("Failed");
  // The reason of a failure and the sharing warning are written in their rows, not behind a mark.
  expect(rowText("Odd")).toContain("rel");
  expect(rowText("Work")).toContain("The account was created, but sharing the default configuration with it could not be turned on.");
  // The sentence says it once: the failure's own summary under it would repeat it, and the account's name.
  expect(rowText("Work")).not.toContain("Could not finish the sharing change");
  expect(card.querySelector("button[data-status-mark]")).toBeNull();
  expect(text).toContain("The agent Codex was added to your agents.");
  expect(text).toContain("The default account of Claude Code is now named as in the file.");
  expect(text).toContain("The new accounts are not logged in yet.");
  expect(Array.from(document.body.querySelectorAll("button")).map((button) => button.textContent)).not.toContain("Log in…");
});
