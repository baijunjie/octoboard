// @vitest-environment jsdom
import { Description, Input, Label, TextField } from "@heroui/react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, onTestFinished } from "vitest";

import { ErrorAlerts } from "../dialogs/ErrorAlerts";
import { errorAlerts } from "../errorAlerts";
import { reportError } from "../reportError";
import { daemonMessage, NO_SUMMARY_CODES } from "../daemonMessage";
import { DaemonRequestError } from "../daemon-client";
import { format, isMessageKey } from "../i18n/catalog";
import { RecoveryGuidance, recoveryParams } from "./ErrorGuidance";
import { errorPresentation, hasCodeParams } from "../errorPresentation";
import { ActionableError } from "../actionableError";
import { useDialogAction } from "../dialogs/Dialog";
import { currentLanguage, setLanguage } from "../i18n/language";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { DaemonErrorMessage } from "./DaemonErrorMessage";
import { hasErrorDetails, hasErrorRemedy } from "./errorPartPresence";
import { ErrorAlert } from "./ErrorAlert";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const params = { shell: "/bin/zsh", command: "env -0", timeout: "10s" };
const failure = () => new DaemonRequestError("plain diagnostic for notifications", "shell_environment_timeout", params);

function mount(element: React.ReactNode): HTMLElement {
  const previousLanguage = currentLanguage();
  act(() => setLanguage("en"));
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
    act(() => setLanguage(previousLanguage));
  });
  act(() => root.render(element));
  return container;
}

/** The whole message and, apart from it, its raw part, as the error alert dialog and a toast lay them out. */
function mountApart(error: DaemonRequestError, daemon: Daemon = { store: createStateStore({}) } as Daemon): { body: Element; raw: Element } {
  const container = mount(
    <DaemonProvider value={daemon}>
      <section data-body><DaemonErrorMessage error={error} /></section>
      <section data-raw><DaemonErrorMessage error={error} part="raw" /></section>
    </DaemonProvider>,
  );
  return { body: container.querySelector("[data-body]")!, raw: container.querySelector("[data-raw]")! };
}

it("retains the structured request failure through the dialog action and responds to language changes", async () => {
  let run!: (action: () => Promise<void>) => Promise<void>;
  function Subject() {
    const action = useDialogAction(undefined, "Could not save");
    run = action.run;
    return null;
  }
  mount(<><Subject /><ErrorAlerts /></>);
  const error = failure();
  await act(() => run(() => Promise.reject(error)));
  const alert = document.querySelector('[role="alertdialog"]');
  expect(document.querySelectorAll('[role="alertdialog"]')).toHaveLength(1);
  expect(alert?.textContent).toContain("Shell environment timed out");
  expect(alert?.querySelector("h3")?.textContent).toBe("Shell environment timed out");
  expect(alert?.querySelector("code")?.textContent).toBe("/bin/zsh -l -i -c 'env -0'");
  expect(error.message).toBe("plain diagnostic for notifications");
  act(() => setLanguage("zh-Hans"));
  expect(alert?.textContent).not.toContain("Shell environment timed out");
  expect(alert?.querySelector("code")?.textContent).toBe("/bin/zsh -l -i -c 'env -0'");
});

it("keeps the complete field description registered when its command uses the code component", () => {
  const container = mount(
    <TextField>
      <Label>Model</Label>
      <Input />
      <Description elementType="div"><DaemonErrorMessage error={failure()} /></Description>
    </TextField>,
  );
  const descriptionId = container.querySelector("input")?.getAttribute("aria-describedby");
  expect(descriptionId).toBeTruthy();
  const description = document.getElementById(descriptionId!);
  expect(description?.textContent).toContain("Shell environment timed out");
  expect(description?.textContent).toContain("10s");
  expect(description?.querySelector("code")?.getAttribute("dir")).toBe("ltr");
  expect(description?.querySelector("code")?.id).not.toBe(descriptionId);
});

it("says the generic sentence for an unfamiliar code, its own text only in the raw part, and the plain fallback for an incomplete shell timeout", () => {
  const unknown = new DaemonRequestError("new daemon error", "future_code", params);
  const incomplete = new DaemonRequestError("incomplete daemon error", "shell_environment_timeout", { shell: "/bin/zsh" });
  expect(errorPresentation(unknown)).toBe(unknown);
  expect(errorPresentation(incomplete)).toBe(incomplete);
  const container = mount(<><DaemonErrorMessage error={unknown} /><DaemonErrorMessage error={incomplete} /></>);
  expect(container.textContent).toBe("Something unexpected went wrong.incomplete daemon error");
  expect(container.querySelector("code")).toBeNull();
  const { body, raw } = mountApart(unknown);
  expect(body.textContent).toBe("Something unexpected went wrong.");
  expect(raw.querySelector("pre")?.textContent).toBe("new daemon error");
});

it("sets the path of a conversation that was not found as code, not as quoted text, and does not repeat the ID", () => {
  const error = new DaemonRequestError("plain", "conversation_to_resume_not_found", { agent: "Claude Code", path: "/Users/dev/.claude", id: "3f2b8c1e" });
  expect(errorPresentation(error)).toBe(error);
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(Array.from(container.querySelectorAll("code"), (code) => code.textContent)).toEqual(["/Users/dev/.claude"]);
  // One element, so an alert's flex column cannot split the sentence into an item per literal.
  expect(container.children).toHaveLength(1);
  expect(container.textContent).toBe("No Claude Code conversation with this ID was found in /Users/dev/.claude.Check the ID, and that it was started under this account.");
});

it("sets resolved project and session names in chips without changing their typeface", () => {
  const daemon = { store: createStateStore({ sessions: new Map([["s1", { id: "s1", title: "Live title" } as import("../protocol").Session]]) }) } as Daemon;
  const error = new DaemonRequestError("plain", "conversation_in_session", { session: "s1" });
  expect(errorPresentation(error)).toBe(error);
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(container.querySelector("[data-inline-reference]")?.textContent).toBe("Live title");
  expect(container.querySelector("code")).toBeNull();
  expect(container.textContent).not.toMatch(/[“”]/);
  act(() => daemon.store.setState({ sessions: new Map([["s1", { id: "s1", title: "Renamed" } as import("../protocol").Session]]) }));
  expect(container.querySelector("[data-inline-reference]")?.textContent).toBe("Renamed");
});

it("keeps a failed launch's name and its nested failure's path as distinct chips", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const params = { title: "Draft", reason_code: "path_not_found", path: "/missing/project" };
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={new DaemonRequestError("fallback", "launch_failed", params)} /></DaemonProvider>);
  expect(Array.from(container.querySelectorAll("[data-inline-reference]"), (chip) => chip.textContent)).toEqual(["Draft", "/missing/project"]);
  expect(container.querySelector("code")?.textContent).toBe("/missing/project");
  expect(container.textContent).toBe("Could not start Draft: /missing/project does not exist.");
});

it("renders structured running-session IDs as distinct live names, preserving commas and quote characters in titles", () => {
  const titles = ["Draft, review “notes”", "Second, “task”"];
  const daemon = { store: createStateStore({ sessions: new Map(titles.map((title, index) => [`s${index}`, { id: `s${index}`, title } as import("../protocol").Session])) }) } as Daemon;
  const params = { count: "3", sessions: "old fallback", session_ids: JSON.stringify(["s0", "s1", "gone"]) };
  const error = new DaemonRequestError("plain fallback", "session_has_running_sessions", params);
  expect(errorPresentation(error)).toBe(error);
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(Array.from(container.querySelectorAll("[data-inline-reference]"), (chip) => chip.textContent)).toEqual([...titles, "gone"]);
  expect(container.querySelector("code")?.textContent).toBe("gone");
  expect(container.textContent).not.toContain("old fallback");
  act(() => daemon.store.setState({ sessions: new Map([["s0", { id: "s0", title: "Renamed" } as import("../protocol").Session]]) }));
  expect(Array.from(container.querySelectorAll("[data-inline-reference]"), (chip) => chip.textContent)).toEqual(["Renamed", "s1", "gone"]);
});

it("does not try to parse old or malformed running-session lists out of quoted title text", () => {
  const params = { count: "2", sessions: "“One, task”, “Other”" };
  for (const extra of [{}, { session_ids: "not JSON" }, { session_ids: '["s0",5]' }, { session_ids: "[]" }] as import("../protocol").MessageParams[]) {
    const error = new DaemonRequestError("old daemon fallback", "session_has_running_sessions", { ...params, ...extra });
    expect(errorPresentation(error)).toBe(error);
    const container = mount(<DaemonErrorMessage error={error} />);
    expect(container.textContent).toBe("old daemon fallback");
    expect(container.querySelector("[data-inline-reference]")).toBeNull();
  }
});

it("names each branch or worktree still there as code with its reason, a comma in a name included", () => {
  const items = [
    { kind: "worktree", name: "/home/dev/wt, two", branch: "a", state: "locked", uncommitted: true, detail: null },
    { kind: "branch", name: "fix/a,b", branch: "fix/a,b", state: "failed", uncommitted: false, detail: "error: refused" },
    { kind: "branch", name: "long", branch: "long", state: "failed", uncommitted: false, detail: "error: one\nhint: two" },
  ];
  const error = new DaemonRequestError("plain fallback", "session_branches_not_removed", { count: "3", items: JSON.stringify(items) });
  const daemon = { store: createStateStore({ homeDir: "/home/dev" }) } as Daemon;
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} part="details" /></DaemonProvider>);
  expect(Array.from(container.querySelectorAll("p"), (lead) => lead.textContent)).toEqual(["Removing these failed:", "This worktree is locked:"]);
  const announced = (row: Element) => {
    const copy = row.cloneNode(true) as Element;
    copy.querySelectorAll("[aria-hidden=true]").forEach((hidden) => hidden.remove());
    // Screen readers skip the bidi isolates around substituted values.
    return copy.textContent?.replace(/[\u2066-\u2069]/g, "");
  };
  expect(Array.from(container.querySelectorAll("li"), announced)).toEqual([
    "fix/a,berror: refused",
    "longerror: one\nhint: two",
    "~/wt, two on branch a, uncommitted changes",
  ]);
  // Git's own messages are code blocks, never chips inside the sentence.
  expect(Array.from(container.querySelectorAll("pre"), (block) => block.textContent)).toEqual(["error: refused", "error: one\nhint: two"]);
  // The names shown in the list are plain monospace text; only the uncommitted tag is a badge.
  expect(container.querySelector("[data-inline-reference]")).toBeNull();
  expect(Array.from(container.querySelectorAll("li .font-mono"), (name) => name.textContent)).toEqual(["fix/a,b", "long", "~/wt, two", "a"]);
  expect(Array.from(container.querySelectorAll("[data-variant]"), (badge) => badge.textContent)).toEqual(["uncommitted changes"]);
  expect(container.querySelector("[data-variant]")?.getAttribute("data-color")).toBe("warning");
  // The badge ends the row's line in a wrapper of its own, after the name, with the comma only a screen reader hears.
  const badge = container.querySelector("[data-variant]")!;
  expect(badge.parentElement?.className).toContain("flex-wrap");
  expect(badge.previousElementSibling?.className).toBe("sr-only");
  expect(badge.parentElement?.firstElementChild?.textContent).toContain("~/wt, two");
  // Where the raw text is kept apart, the list names each and git's messages are the raw part, each under its name.
  const { body, raw } = mountApart(error, daemon);
  expect(Array.from(body.querySelectorAll("li"), announced)).toEqual(["fix/a,b", "long", "~/wt, two on branch a, uncommitted changes"]);
  expect(body.querySelector("pre")).toBeNull();
  expect(Array.from(raw.querySelectorAll("h3"), (name) => [name.textContent, name.nextElementSibling?.querySelector("pre")?.textContent])).toEqual([
    ["fix/a,b", "error: refused"],
    ["long", "error: one\nhint: two"],
  ]);
});

it("keeps a sharing repair's own reason behind Details when it names no entry, and lists the entries there once it does", () => {
  const daemon = { store: createStateStore({ sessions: new Map([["s1", { id: "s1", title: "Hub" } as import("../protocol").Session]]) }) } as Daemon;
  const reason = "account `Work`'s links to the default configuration could not be checked: permission denied";
  const empty = new DaemonRequestError(reason, "account_sharing_repair_failed", { account: "a1", name: "Work", items: "[]", cause: "permission_denied" });
  const unnamed = mountApart(empty, daemon);
  // Alone on its line it is a diagnostic block on its own surface, not a chip, and only in the raw part.
  expect(unnamed.raw.querySelector("pre code")?.textContent).toBe(reason);
  expect(unnamed.raw.querySelector("[data-inline-reference]")).toBeNull();
  expect(unnamed.body.textContent).toContain("could not be brought up to date");
  expect(unnamed.body.textContent).toContain("You do not have permission to change the account’s directory.");
  expect(unnamed.body.textContent).not.toContain(reason);
  const items = JSON.stringify([{ item: "skills/", left: "unchanged", detail: "permission denied", cause: "busy" }]);
  const listed = mountApart(new DaemonRequestError(reason, "account_sharing_repair_failed", { account: "a1", name: "Work", items }), daemon);
  expect(listed.body.textContent).toContain("skills/");
  expect(listed.body.textContent).toContain("is in use");
  expect(listed.body.textContent).not.toContain(reason);
  expect(listed.body.textContent).not.toContain("permission denied");
  expect(listed.raw.querySelector("li")?.lastElementChild?.textContent).toBe("permission denied");
  expect(listed.raw.textContent).toContain("skills/ was left unchanged.");
});

it("keeps the reason what goes with a removed project's folder is unknown as a code block apart from the sentence", () => {
  const error = new DaemonRequestError("plain fallback", "project_branches_not_determined", { name: "Docs", detail: "fatal: not a git repository" });
  const { body, raw } = mountApart(error);
  expect(raw.querySelector("pre")?.textContent).toBe("fatal: not a git repository");
  expect(body.textContent).not.toContain("fatal");
  expect(body.querySelector("pre")).toBeNull();
});

it("leaves the line over the groups out when there is one group, whose lead says it all", () => {
  const items = [{ kind: "branch", name: "fix/a", branch: "fix/a", state: "unmerged", uncommitted: false, detail: null }];
  const error = new DaemonRequestError("plain fallback", "session_branches_not_removed", { count: "1", items: JSON.stringify(items) });
  const container = mount(<DaemonProvider value={{ store: createStateStore({}) } as Daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(container.textContent).not.toContain("still there");
  expect(container.querySelector("p")?.textContent).toBe("This branch is not merged:");
});

// The split is a rendering boundary: every summary fits Alert.Description while the rest remains
// readable exactly once outside it, including a failure nested in another failure's sentence.
it.each([
  { name: "a simple structured reason", error: new DaemonRequestError("fallback", "path_not_found", { path: "/missing" }), summary: "/missing does not exist.", detail: undefined },
  { name: "multiline diagnostics", error: new DaemonRequestError("fallback", "project_branches_not_determined", { name: "Docs", detail: "fatal: one\nhint: two" }), summary: "Docs", detail: "fatal: one\nhint: two" },
  { name: "nested diagnostics", error: new DaemonRequestError("fallback", "launch_failed", { title: "Draft", reason_code: "project_branches_not_determined", name: "Docs", detail: "fatal: nested\nhint: two" }), summary: "Could not start Draft:", detail: "fatal: nested\nhint: two" },
  { name: "a shell timeout", error: failure(), summary: "A shell startup file is probably blocked", detail: "/bin/zsh -l -i -c 'env -0'" },
  { name: "a nested shell timeout", error: new DaemonRequestError("fallback", "launch_failed", { title: "Draft", reason_code: "shell_environment_timeout", ...params }), summary: "Could not start Draft: A shell startup file is probably blocked", detail: "/bin/zsh -l -i -c 'env -0'" },
  { name: "a branch group", error: new DaemonRequestError("fallback", "session_branches_not_removed", { count: "1", items: JSON.stringify([{ kind: "branch", name: "fix/a", branch: "fix/a", state: "failed", detail: "error: refused" }]) }), summary: "Removing this failed:", detail: "error: refused" },
])("keeps $name in inline summary and external details without duplication", ({ error, summary, detail }) => {
  const container = mount(<DaemonProvider value={{ store: createStateStore({}) } as Daemon}>
    <ErrorAlert title="Could not finish"><DaemonErrorMessage error={error} part="summary" /></ErrorAlert>
    <section><DaemonErrorMessage error={error} part="details" /></section>
  </DaemonProvider>);
  const description = container.querySelector('[data-slot="alert-description"]')!;
  const details = container.querySelector("section")!;
  expect(description.textContent).toContain(summary);
  expect(description.querySelector("div,p,ul,li,pre,h1,h2,h3")).toBeNull();
  expect(details.textContent).not.toContain(summary);
  if (detail) {
    expect(description.textContent).not.toContain(detail);
    expect(details.textContent).toContain(detail);
    expect(container.textContent?.split(detail)).toHaveLength(2);
  } else expect(details.textContent).toBe("");
});

it.each([
  ["console", "consoles", "name"], ["project", "projects", "name"],
  ["session", "sessions", "title"], ["shell_terminal", "shellTerminals", "title"],
  ["account", "accounts", "name"],
])("resolves %s names and missing IDs with distinct rich direction and plain text boundaries", (field, collection, label) => {
  const title = "مراجعة / Review, 名称";
  const entries = [{ id: "known", [label]: title }];
  const state = collection === "accounts" ? { settings: { accounts: entries } } : { [collection]: new Map(entries.map((entry) => [entry.id, entry])) };
  const store = createStateStore(state as Parameters<typeof createStateStore>[0]);
  const error = new DaemonRequestError("fallback", `unknown_${field}`, { [field]: "known" });
  const container = mount(<DaemonProvider value={{ store } as Daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  const chip = () => container.querySelector("[data-inline-reference]")!;
  expect(chip().textContent).toBe(title);
  expect(chip().getAttribute("dir")).toBe("auto");
  expect(chip().querySelector("code")).toBeNull();
  expect(chip().textContent).not.toMatch(/[\u2066-\u2069]/);
  expect(daemonMessage("en", error.code, error.params, error.message, store.getState())).toContain(`\u2068${title}\u2069`);
  act(() => store.setState({ ...store.getState(), ...(collection === "accounts" ? { settings: { ...store.getState().settings, accounts: [] } } : { [collection]: new Map() }) }));
  expect(chip().textContent).toBe("known");
  expect(chip().getAttribute("dir")).toBe("ltr");
  expect(chip().querySelector("code")?.textContent).toBe("known");
  expect(daemonMessage("en", error.code, error.params, error.message, store.getState())).toContain("\u2068known\u2069");
});

it.each([
  ["unknown_page", { page: "page-id" }, ["page-id"]],
  ["field_required", { field: "account_id" }, ["account_id"]],
  ["field_too_long", { field: "session_id", limit: "100" }, ["session_id"]],
  ["conflicting_fields", { first: "model", second: "effort" }, ["model", "effort"]],
  ["invalid_launch_setting", { value: "xhigh", field: "effort", agent: "Codex" }, ["xhigh", "effort", "Codex"]],
] as const)("marks schema literals and technical fallback in %s without contaminating plain text", (code, params, chips) => {
  const error = new DaemonRequestError("fallback", code, params);
  const container = mount(<DaemonErrorMessage error={error} />);
  expect(Array.from(container.querySelectorAll("[data-inline-reference]"), (chip) => chip.textContent)).toEqual(chips);
  const plain = daemonMessage("en", code, params, error.message, createStateStore({}).getState());
  for (const value of chips) expect(plain).toContain(`\u2068${value}\u2069`);
  expect(plain).not.toMatch(/<\/?(?:span|code)/);
});

it.each([
  "project_branches_not_determined", "git_failed", "git_clone_failed", "git_remote_unreachable",
  "git_unavailable",
  "models_command_failed", "models_shell_environment_failed", "quota_command_failed", "quota_shell_environment_failed",
  "codex_unarchive_failed", "login_failed",
])("keeps %s diagnostics byte-for-byte once outside rich summaries and in plain text", (code) => {
  const detail = "fatal: /tmp/مراجعة/one\n    indented diagnostic\n" + "x".repeat(400);
  const params = { detail, agent: "Codex", name: "Docs" };
  const error = new DaemonRequestError("fallback", code, params);
  const container = mount(<><DaemonErrorMessage error={error} part="summary" /><section><DaemonErrorMessage error={error} part="details" /></section></>);
  expect(container.querySelector("pre")?.textContent).toBe(detail);
  expect(container.querySelectorAll("pre")).toHaveLength(1);
  expect(container.querySelector("pre")?.getAttribute("dir")).toBe("ltr");
  expect(container.querySelector("section")?.textContent).toBe(detail);
  expect(container.firstChild?.textContent).not.toContain("fatal");
  const plain = daemonMessage("en", code, params, error.message, createStateStore({}).getState());
  expect(plain).toContain(detail);
  expect(plain.split(detail)).toHaveLength(2);
});

it("keeps a complete nested diagnostic outside its launch sentence and translates name references", () => {
  const error = new DaemonRequestError("fallback", "launch_failed", { title: "Draft", reason_code: "quota_command_failed", agent: "Codex", detail: "raw\n    quota failure" });
  const { body, raw } = mountApart(error);
  expect(body.querySelector("span")?.textContent).toBe("Could not start Draft: Codex could not report its usage.");
  expect(body.querySelector("pre")).toBeNull();
  expect(raw.querySelectorAll("pre")).toHaveLength(1);
  act(() => setLanguage("zh-Hans"));
  expect(body.textContent).toContain("无法报告用量");
  expect(raw.querySelector("pre")?.textContent).toBe("raw\n    quota failure");
});

it.each(["unreadable_request", "project_directory_trash_failed", "configuration_unreadable"])("says %s in its own sentence, the system's text only in the raw part", (code) => {
  const detail = "Operation not permitted (os error 1)";
  const { body, raw } = mountApart(new DaemonRequestError("fallback", code, { detail }));
  expect(body.textContent).toBe(format("en", `daemon.${code}.summary` as never));
  expect(body.textContent).not.toContain(detail);
  expect(raw.querySelector("pre")?.textContent).toBe(detail);
});

it.each([...NO_SUMMARY_CODES])("gives %s no first line, its title saying what failed, and keeps the system's text only in the raw part", (code) => {
  const detail = "Operation not permitted (os error 1)";
  const error = new DaemonRequestError("fallback", code, { detail });
  const { body, raw } = mountApart(error);
  expect(isMessageKey(`daemon.${code}.summary`)).toBe(false);
  expect(mount(<DaemonErrorMessage error={error} part="summary" />).textContent).toBe("");
  expect(body.textContent).toBe("");
  expect(raw.querySelector("pre")?.textContent).toBe(detail);
  expect(daemonMessage("en", code, { detail }, "fallback", createStateStore({}).getState())).toContain(detail);
  expect(daemonMessage("en", code, { detail }, "fallback", createStateStore({}).getState(), true)).toBe("");
});

it.each(["git_restore_failed", "terminal_unavailable"])("shows %s with no detail as nothing of its own: no first line, no dangling colon, no Error details", (code) => {
  const error = new DaemonRequestError(`plain message for ${code}: `, code, { detail: "" });
  const { body, raw } = mountApart(error);
  expect(hasCodeParams(error)).toBe(true);
  expect(body.textContent).toBe("");
  expect(raw.textContent).toBe("");
  expect(hasErrorDetails(error)).toBe(false);
  expect(hasErrorRemedy(error)).toBe(false);
});

it("raises git_switch_failed as an alert titled for what failed, its body starting with the remedy and the text only under Error details", () => {
  const detail = "error: Your local changes would be overwritten by checkout";
  reportError(new DaemonRequestError("fallback", "git_switch_failed", { detail }), undefined, undefined, undefined, "Could not switch branches");
  mount(<ErrorAlerts />);
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.querySelector("h2")?.textContent).toBe("Could not switch branches");
  const body = alert.querySelector("[class*='empty:hidden']")!;
  expect(body.childNodes).toHaveLength(0);
  expect(body.nextElementSibling?.textContent).toBe(format("en", "error.recovery.gitSwitchFailure", { terminal: "Terminal", git: "Git" }));
  expect(alert.querySelector("pre")?.textContent).toBe(detail);
  expect(alert.textContent).not.toContain("could not switch");
});

it.each([
  { case: "a launch whose failure is internal", error: new DaemonRequestError("fallback", "launch_failed", { title: "Draft", reason_code: "internal_error", reason: "spawn failed: os error 2", detail: "spawn failed: os error 2" }), sentence: "Could not start Draft: Something unexpected went wrong.", rawText: "spawn failed: os error 2" },
  { case: "a launch whose failure the catalog lacks", error: new DaemonRequestError("fallback", "launch_failed", { title: "Draft", reason_code: "future_code", reason: "a newer daemon's reason" }), sentence: "Could not start Draft: Something unexpected went wrong.", rawText: "a newer daemon's reason" },
  { case: "a launch whose uncatalogued failure the system's text names", error: new DaemonRequestError("fallback", "launch_failed", { title: "Draft", reason_code: "future_code", reason: "No space left on device (os error 28)" }), sentence: "Could not start Draft: The disk is full.", rawText: "No space left on device (os error 28)" },
  { case: "a trust answer whose reason the catalog lacks", error: new DaemonRequestError("fallback", "trust_answer_failed", { agent: "Codex", reason_code: "future_reason", reason: "the daemon's English reason" }), sentence: "Octoboard could not press Codex's trust confirmation (something unexpected went wrong).", rawText: "the daemon's English reason" },
])("says $case with a generic reason, its own text only in the raw part", ({ error, sentence, rawText }) => {
  const { body, raw } = mountApart(error);
  expect(body.querySelector("span")?.textContent?.replace(/[\u2066-\u2069]/g, "")).toBe(sentence);
  expect(body.textContent).not.toContain(rawText);
  expect(raw.querySelector("pre")?.textContent).toBe(rawText);
  expect(hasErrorDetails(error)).toBe(true);
});

describe("a failed automatic account switch", () => {
  const params = { agent: "Codex", from: "Work", account: "Spare", reason_code: "path_not_found", path: "/missing/project" };

  it("says what happened, then why on a line of its own with the nested failure's own chips", () => {
    const daemon = { store: createStateStore({}) } as Daemon;
    const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={new DaemonRequestError("fallback", "account_switch_automatically_failed", params)} /></DaemonProvider>);
    expect(Array.from(container.querySelectorAll("[data-inline-reference]:not(:has(code))"), (chip) => chip.textContent)).toEqual(expect.arrayContaining(["Work", "Codex", "Spare"]));
    expect(container.querySelector("code")?.textContent).toBe("/missing/project");
    const text = container.textContent?.replace(/[\u2066-\u2069]/g, "");
    expect(text).toContain("nor bring it back up on");
    expect(text).not.toMatch(/retry|try again/i);
    expect(text).toContain("/missing/project does not exist.");
  });

  it("leaves the nested failure's own remedy to the sentence's, which a failed launch's does not have", () => {
    const nested = { reason_code: "config_dir_unreachable", agent: "Codex", path: "/missing" };
    const switched = new DaemonRequestError("fallback", "account_switch_automatically_failed", { ...params, ...nested });
    const launched = new DaemonRequestError("fallback", "launch_failed", { title: "Draft", ...nested });
    expect(mount(<DaemonErrorMessage error={launched} />).textContent).toContain("Recreate it");
    const text = mount(<DaemonErrorMessage error={switched} />).textContent;
    expect(text).toContain("/missing");
    expect(text).not.toContain("Recreate it");
    expect(hasErrorRemedy(switched)).toBe(false);
    expect(hasErrorRemedy(launched)).toBe(true);
  });

  it("says a reason the catalog lacks as the generic sentence, its own text only in the raw part", () => {
    const error = new DaemonRequestError("fallback", "account_switch_automatically_failed", { ...params, reason_code: "future_code", reason: "a newer daemon's reason" });
    const { body, raw } = mountApart(error);
    expect(body.textContent).toContain("Something unexpected went wrong.");
    expect(body.textContent).not.toContain("a newer daemon's reason");
    expect(raw.querySelector("pre")?.textContent).toBe("a newer daemon's reason");
    expect(hasErrorDetails(error)).toBe(true);
  });
});

it("uses name chips for fixed agent and product references while leaving plain messages usable", () => {
  const error = new DaemonRequestError("fallback", "agent_not_available", { agent: "Codex" });
  const container = mount(<DaemonErrorMessage error={error} />);
  expect(Array.from(container.querySelectorAll("[data-inline-reference]:not(:has(code))"), (chip) => chip.textContent)).toEqual(["Codex", "Octoboard", "Claude Code", "Codex", "Grok Build"]);
  const plain = daemonMessage("en", error.code, error.params, error.message, createStateStore({}).getState());
  expect(plain.replace(/[\u2066-\u2069]/g, "")).toContain("supports Claude Code, Codex and Grok Build");
});

it("retains a named quota window as a rich name and a plain isolated value", () => {
  const params = { agent: "Codex", account: "Work", window: "模型 / Model weekly" };
  const container = mount(<DaemonErrorMessage error={new DaemonRequestError("fallback", "account_quota_exhausted", params)} />);
  expect(Array.from(container.querySelectorAll("[data-inline-reference]"), (chip) => chip.textContent)).toEqual(["Work", "Codex", params.window]);
  expect(daemonMessage("en", "account_quota_exhausted", params, "fallback", createStateStore({}).getState())).toContain(`\u2068${params.window}\u2069`);
});

it.each(["path_not_absolute", "project_directory_shared", "agent_not_available", "console_has_running_sessions", "trust_answer_failed", "conversation_in_session", "session_locked", "queued_messages_lost"])("keeps %s recovery visible as the remedy, with only its own further explanation for Details, and preserves the complete plain wording", (code) => {
  const params = { path: "/tmp/مراجعة", project: "Docs", agent: "Codex", reason_code: "screen_gone", session: "missing", reason: "fallback reason" };
  const error = new DaemonRequestError("fallback", code, params);
  const container = mount(<><section data-summary><DaemonErrorMessage error={error} part="summary" /></section><section data-remedy><DaemonErrorMessage error={error} part="remedy" /></section><section data-details><DaemonErrorMessage error={error} part="details" /></section></>);
  const summary = container.querySelector("[data-summary]")!;
  const remedy = container.querySelector("[data-remedy]")!;
  const details = container.querySelector("[data-details]")!;
  const normalize = (text: string | null) => text?.replace(/[\u2066-\u2069]/g, "").replace(/\s/g, "");
  const plain = daemonMessage("en", code, params, error.message, createStateStore({}).getState());
  expect(summary.querySelector("div,p,ul,pre")).toBeNull();
  expect(remedy.textContent).not.toBe("");
  expect(remedy.textContent).not.toContain(summary.textContent);
  // The one with a `.details` line has it, and only it, there; the plain sentence runs summary, further explanation, remedy.
  const further = code === "queued_messages_lost";
  expect(details.textContent === "").toBe(!further);
  expect(hasErrorDetails(error)).toBe(further);
  expect(hasErrorRemedy(error)).toBe(true);
  const whole = () => normalize((summary.textContent ?? "") + details.textContent + remedy.textContent);
  expect(whole()).toBe(normalize(plain));
  act(() => setLanguage("zh-Hans"));
  expect(whole()).toBe(normalize(daemonMessage("zh-Hans", code, params, error.message, createStateStore({}).getState())));
});

it("retains local quota-refusal action metadata in rich copy and a plain fallback", () => {
  const error = new DaemonRequestError("plain fallback", "quick_create_quota_refused", { action: "New session with defaults" });
  const container = mount(<DaemonErrorMessage error={error} />);
  expect(container.querySelector("[data-inline-reference]")?.textContent).toBe("New session with defaults");
  expect(error.message).toBe("plain fallback");
});

it.each(["terminal_write_failed", "store_io_failed"])("keeps nested trust %s diagnostics outside the reason sentence", (reason_code) => {
  const detail = "Permission denied\n    /tmp/مراجعة";
  const error = new DaemonRequestError("fallback", "trust_answer_failed", { agent: "Codex", reason_code, detail });
  const { body, raw } = mountApart(error);
  expect(body.textContent).not.toContain(detail);
  expect(body.querySelector("pre")).toBeNull();
  expect(raw.querySelector("pre")?.textContent).toBe(detail);
  expect(raw.querySelectorAll("pre")).toHaveLength(1);
  expect(body.textContent).toContain("Answer it in the terminal.");
});

it("retains the login producer's detail-only shell failure without inventing a command", () => {
  const detail = "capturing environment failed\n    permission denied";
  const fallback = `The shell environment could not be captured: ${detail}`;
  const error = new DaemonRequestError(fallback, "shell_environment_timeout", { detail });
  const { body, raw } = mountApart(error);
  expect(body.textContent).toBe("The shell environment could not be captured.");
  expect(raw.querySelector("pre")?.textContent).toBe(detail);
  expect(body.textContent).not.toMatch(/\{(?:shell|command|timeout)\}/);
  expect(daemonMessage("en", error.code, error.params, fallback, createStateStore({}).getState())).toBe(fallback);
});

it.each(["project", "console"])("preserves the held %s session's task and name as one raw directed chip", (owner) => {
  const session = { id: "held", title: "مراجعة name", terminal_title: "  Task / 任务  " } as import("../protocol").Session;
  const store = createStateStore({ sessions: new Map([[session.id, session]]), projects: new Map([["owner", { id: "owner", name: "Owner" } as import("../protocol").Project]]), consoles: new Map([["owner", { id: "owner", name: "Owner" } as import("../protocol").Console]]) });
  const params = { session: session.id, [owner]: "owner" };
  const error = new DaemonRequestError("fallback", `conversation_held_in_${owner}`, params);
  const container = mount(<DaemonProvider value={{ store } as Daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  const chips = [...container.querySelectorAll("[data-inline-reference]")];
  expect(chips.map((chip) => chip.textContent)).toEqual(["Task / 任务｜" + session.title, "Owner"]);
  expect(chips.every((chip) => chip.getAttribute("dir") === "auto" && !/[\u2066-\u2069]/.test(chip.textContent ?? ""))).toBe(true);
  // One chip around the joined title, each name isolated at element level so mixed directions
  // cannot reorder across the separator.
  expect([...chips[0].querySelectorAll("bdi")].map((name) => name.textContent)).toEqual(["Task / 任务", session.title]);
  for (const language of ["en", "zh-Hans"] as const) {
    const plain = daemonMessage(language, error.code, params, error.message, store.getState());
    expect(plain).toContain("\u2068Task / 任务\u2069｜\u2068مراجعة name\u2069");
  }
  act(() => store.setState({ ...store.getState(), sessions: new Map([[session.id, { ...session, terminal_title: session.title }]]) }));
  expect([...container.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toEqual([session.title, "Owner"]);
});

it.each([
  ["tool", ["Git", "PATH", "Octoboard"]],
  ["projectBranches", ["Terminal", "git worktree list", "git branch", "Git"]],
  ["sessionBranches", ["Terminal", "Git"]],
  ["repository", ["Git", "Git", "Terminal"]],
  ["noCommits", ["Terminal"]], ["shell", ["Terminal"]],
  ["gitFailure", ["Terminal", "Git", "Git"]], ["gitSwitchFailure", ["Terminal", "Git", "Git"]], ["gitRestoreFailure", ["Terminal", "Git"]], ["gitHistory", ["Terminal"]],
  ["unsentMessage", ["Enter"]],
] as const)("marks %s recovery references while preserving a matching plain companion", (recovery, englishValues) => {
  const container = mount(<RecoveryGuidance recovery={recovery} />);
  for (const language of ["en", "zh-Hans"] as const) {
    act(() => setLanguage(language));
    const chips = [...container.querySelectorAll("[data-inline-reference]")];
    const expected = language === "zh-Hans" && recovery === "repository" ? ["Git", "终端", "Git"] : englishValues.map((value) => language === "zh-Hans" && value === "Terminal" ? "终端" : value);
    expect(chips.map((chip) => chip.textContent)).toEqual(expected);
    for (const chip of chips) {
      const technical = ["PATH", "Enter", "git worktree list", "git branch"].includes(chip.textContent!);
      expect(chip.getAttribute("dir")).toBe(technical ? "ltr" : "auto");
      expect(!!chip.querySelector("code")).toBe(technical);
      expect(chip.textContent).not.toMatch(/[\u2066-\u2069]/);
    }
    const plain = format(language, `error.recovery.${recovery}`, recoveryParams((key, ...args) => format(language, key, args[0])));
    expect(container.textContent).toBe(plain);
    expect(plain).not.toMatch(/\{\w+\}|<\/?(?:span|code)/);
  }
});

it("keeps generic terminal nouns as prose rather than treating them as named controls", () => {
  const container = mount(<RecoveryGuidance recovery="stopProcess" />);
  expect(container.textContent).toBe("Open the affected session’s terminal and exit the agent using its quit command, then retry.");
  expect(container.querySelector("[data-inline-reference]")).toBeNull();
});

it("routes the operating-system reference in directory recovery through the shared rich/plain boundary", () => {
  const params = { path: "/Volumes/مراجعة", detail: "permission denied" };
  const container = mount(<DaemonErrorMessage error={new DaemonRequestError("fallback", "directory_unreadable", params)} />);
  const system = [...container.querySelectorAll("[data-inline-reference]")].find((chip) => chip.textContent === "macOS")!;
  expect(system.getAttribute("dir")).toBe("auto");
  expect(system.querySelector("code")).toBeNull();
  expect(system.closest("span.block")?.textContent).toContain("asks for permission per volume");
  // The raw text is Details' diagnostic block, and the visible sentence and remedy hold none of it.
  const summary = mount(<DaemonErrorMessage error={new DaemonRequestError("fallback", "directory_unreadable", params)} part="summary" />);
  const remedy = mount(<DaemonErrorMessage error={new DaemonRequestError("fallback", "directory_unreadable", params)} part="remedy" />);
  expect(summary.textContent + remedy.textContent).not.toContain("permission denied");
  expect(mount(<DaemonErrorMessage error={new DaemonRequestError("fallback", "directory_unreadable", params)} part="details" />).textContent).toContain("permission denied");
  const plain = daemonMessage("en", "directory_unreadable", params, "fallback", createStateStore({}).getState());
  expect(plain).toContain("\u2068macOS\u2069 asks for permission per volume");
  expect(plain).not.toMatch(/\{\w+\}/);
});

it("retains a named key inside a structured trust reason as a technical chip", () => {
  const params = { agent: "Codex", reason_code: "cursor_did_not_move" };
  const container = mount(<DaemonErrorMessage error={new DaemonRequestError("fallback", "trust_answer_failed", params)} />);
  expect(container.querySelector("code")?.textContent).toBe("Enter");
  expect(container.querySelector("code")?.getAttribute("dir")).toBe("ltr");
  expect(container.textContent).toContain("so Enter was not sent");
  expect(container.textContent).toContain("Answer it in the terminal.");
  const plain = daemonMessage("en", "trust_answer_failed", params, "fallback", createStateStore({}).getState());
  expect(plain).toContain("so \u2068Enter\u2069 was not sent");
});

it.each(["account_sharing_failed", "account_sharing_repair_failed"])("says %s short by cause at the top of the details part, ahead of the raw diagnostics", (code) => {
  const detail = "permission denied\n    /tmp/مراجعة/file\n";
  const session = { id: "s1", title: "مراجعة / Review, 名称" } as import("../protocol").Session;
  const store = createStateStore({ sessions: new Map([[session.id, session]]) });
  const params = { name: "Work", account: "a1", detail: "do not repeat the daemon summary", items: JSON.stringify([
    { item: "skills/", left: "unchanged", detail, cause: "permission_denied" },
    { item: "plugins/", left: "unchanged", detail: "second", cause: "permission_denied" },
    { item: "conversation-id", left: "partly_moved", detail: "busy", sessions: ["s1", "gone"], cause: "busy" },
  ]) };
  const error = new DaemonRequestError("do not repeat the raw fallback", code, params);
  const container = mount(<DaemonProvider value={{ store } as Daemon}>
    <ErrorAlert title="Could not finish"><DaemonErrorMessage error={error} part="summary" /></ErrorAlert>
    <section><DaemonErrorMessage error={error} part="details" /></section>
  </DaemonProvider>);
  // The causes and their fixes are all further explanation: no main remedy of its own.
  expect(hasErrorRemedy(error)).toBe(false);
  const summary = container.querySelector('[data-slot="alert-description"]')!;
  const details = container.querySelector("section")!;
  for (const language of ["en", "zh-Hans"] as const) {
    act(() => setLanguage(language));
    expect(summary.textContent).toContain("Work");
    expect(summary.querySelector("div,p,ul,li,pre")).toBeNull();
    expect(details.textContent).not.toContain("Work");
    const causes = [...details.querySelectorAll("span.flex.flex-col > span")];
    expect(causes).toHaveLength(2);
    expect(causes[0]!.textContent).toBe(language === "en" ? "You do not have permission to change skills/ and 1 more. Check that the files and folders involved can be changed and are not locked." : "没有权限修改 skills/ 和另外 1 项。请检查相关文件和文件夹的权限，并确认它们未被锁定。");
    expect(causes[1]!.textContent).toContain(session.title);
  }
  // The details part is what an alert's Details popover shows, so it has no disclosure of its own to open.
  expect(details.querySelector("button")).toBeNull();
  expect(details.querySelector("[hidden]")).toBeNull();
  expect(details.querySelectorAll("li")).toHaveLength(3);
  expect(details.firstElementChild!.querySelectorAll("li")).toHaveLength(0);
  // The system's message for an entry is raw text, so it is a block of its own whatever its length, never a chip.
  expect([...details.querySelectorAll("pre")].map((node) => node.textContent)).toEqual([detail, "second", "busy"]);
  expect([...details.querySelectorAll("li")].map((item) => item.lastElementChild!.textContent)).toEqual([expect.stringContaining("permission denied"), "second", "busy"]);
  expect(details.querySelectorAll("li")[2]!.textContent).toContain(session.title);
  expect(container.textContent).not.toContain("do not repeat");
  act(() => store.setState({ sessions: new Map([[session.id, { ...session, title: "Renamed" }]]) }));
  expect(details.textContent).toContain("Renamed");
});

it("names the cause of a change that could not start when no entry is listed", () => {
  const error = new DaemonRequestError("raw", "account_sharing_failed", { name: "Work", account: "a1", items: "[]", detail: "default dir moved", cause: "conflict" });
  const { body, raw } = mountApart(error);
  expect(body.textContent).toContain("The account’s directory conflicts with what sharing needs.");
  expect(body.textContent).not.toContain("default dir moved");
  expect(raw.querySelector("pre code")?.textContent).toBe("default dir moved");
  expect(raw.querySelector("[data-inline-reference]")).toBeNull();
});

it("joins a whole failure's reason and its guidance in the catalog, and points a cause with no line of its own at Details", () => {
  const mountWith = (cause: string, detail: string | undefined) => mount(<DaemonProvider value={{ store: createStateStore({}) } as Daemon}>
    <DaemonErrorMessage error={new DaemonRequestError("raw", "account_sharing_failed", { name: "Work", account: "a1", items: "[]", detail, cause } as never)} />
  </DaemonProvider>);
  const busy = mountWith("busy", "x");
  const other = mountWith("other", "x");
  act(() => setLanguage("zh-Hans"));
  expect(busy.querySelector("span.block")!.textContent).toBe("此账户目录中有内容正在被使用。请关闭正在使用相关文件的其他程序，例如终端中的智能体、编辑器或同步工具。");
  expect(other.querySelector("span.block")!.textContent).toBe("请查看详情。");
  act(() => setLanguage("en"));
  expect(other.querySelector("span.block")!.textContent).toBe("See the details.");
  expect(mountWith("other", undefined).querySelector("span.block")).toBeNull();
});

it.each([
  { code: "account_sharing_failed", items: "[]", original: "original failure", detail: "raw start failure\n    denied" },
  { code: "account_sharing_repair_failed", items: "[]", original: "raw repair failure\n    denied", detail: "not the original repair reason" },
  { code: "account_sharing_repair_failed", items: "invalid JSON", original: "raw malformed repair\n    denied", detail: "not the original repair reason" },
])("keeps $code with $items diagnostics raw and separate", ({ code, items, original, detail }) => {
  const diagnostic = code === "account_sharing_failed" ? detail : original;
  const error = new DaemonRequestError(original, code, { name: "Work", account: "a1", items, detail });
  const container = mount(<><section data-summary><DaemonErrorMessage error={error} part="summary" /></section><section data-details><DaemonErrorMessage error={error} part="details" /></section></>);
  expect(container.querySelector("[data-summary]")?.textContent).toContain("Work");
  expect(container.querySelector("[data-summary]")?.textContent).not.toContain(diagnostic);
  expect(container.querySelector("[data-summary] pre")).toBeNull();
  expect(container.querySelectorAll("pre")).toHaveLength(1);
  expect(container.querySelector("[data-details] pre")?.textContent).toBe(diagnostic);
  expect(container.querySelector("pre")?.getAttribute("dir")).toBe("ltr");
});

it("shows a session left interrupted after a sharing change as a sentence with the reason as a code block", () => {
  const error = new DaemonRequestError("raw", "sharing_resume_failed", { account: "a1", name: "Work", detail: "no such file" });
  const { body, raw } = mountApart(error);
  expect(body.textContent).toContain("could not be resumed after its sharing changed");
  expect(body.querySelector("[data-inline-reference]")?.textContent).toBe("Work");
  expect(body.textContent).not.toContain("no such file");
  expect(raw.querySelector("pre")?.textContent).toBe("no such file");
});

it("shows the still-running variant of the resume notice without 'left interrupted', keeping the reason as a code block", () => {
  const error = new DaemonRequestError("raw", "sharing_resume_failed", { account: "a1", name: "Work", detail: "timed out", still_running: "true" });
  const { body, raw } = mountApart(error);
  expect(body.textContent).toContain("may still be running; it was not restarted");
  expect(body.textContent).not.toContain("interrupted");
  expect(raw.querySelector("pre")?.textContent).toBe("timed out");
});

it.each([
  ["a message with no coded params", "unexpected_error", {}],
  ["a coded message with nothing more to say", "unknown_account", { account: "missing-account" }],
  ["a diagnostic block", "git_failed", { detail: "fatal: not a git repository" }],
  ["a remedy", "session_locked", {}],
  ["a shell timeout", "shell_environment_timeout", params],
  ["a trust reason with a diagnostic", "trust_answer_failed", { agent: "Codex", reason_code: "terminal_write_failed", detail: "EPIPE" }],
  ["a nested cause with a remedy", "launch_failed", { agent: "Codex", reason: "x", reason_code: "agent_not_available" }],
  ["a sharing failure with a reason for the whole change", "account_sharing_failed", { name: "Work", account: "a1", items: "[]", detail: "default dir moved", cause: "conflict" }],
  ["a sharing failure with no reason to give", "account_sharing_failed", { name: "Work", account: "a1", items: "[]", cause: "other" }],
])("tells without rendering whether the details part of %s has anything", (_name, code, failureParams) => {
  const error = new DaemonRequestError("raw", code, failureParams as never);
  const container = mount(<DaemonProvider value={{ store: createStateStore({}) } as Daemon}><DaemonErrorMessage error={error} part="details" /></DaemonProvider>);
  expect(hasErrorDetails(error)).toBe(container.innerHTML !== "");
});

it.each([
  ["a message with no coded params", "unexpected_error", {}],
  ["a coded message with nothing more to say", "unknown_account", { account: "missing-account" }],
  ["a diagnostic block", "git_failed", { detail: "fatal: not a git repository" }],
  ["a recovery", "session_locked", {}],
  ["a recovery and a further explanation", "queued_messages_lost", {}],
  ["a shell timeout", "shell_environment_timeout", params],
  ["a trust reason with a diagnostic", "trust_answer_failed", { agent: "Codex", reason_code: "terminal_write_failed", detail: "EPIPE" }],
  ["a nested cause with a remedy", "launch_failed", { agent: "Codex", reason: "x", reason_code: "agent_not_available" }],
  ["a nested cause with none", "launch_failed", { agent: "Codex", reason: "x", reason_code: "unknown_account", account: "a" }],
  ["a sharing failure", "account_sharing_failed", { name: "Work", account: "a1", items: "[]", detail: "default dir moved", cause: "conflict" }],
])("tells without rendering whether the remedy part of %s has anything", (_name, code, failureParams) => {
  const error = new DaemonRequestError("raw", code, failureParams as never);
  const container = mount(<DaemonProvider value={{ store: createStateStore({}) } as Daemon}><DaemonErrorMessage error={error} part="remedy" /></DaemonProvider>);
  expect(hasErrorRemedy(error)).toBe(container.innerHTML !== "");
});

it("splits a shell timeout into the sentence, the one remedy, and the consequence then the command in Details", () => {
  const error = failure();
  const container = mount(<><section data-summary><DaemonErrorMessage error={error} part="summary" /></section><section data-remedy><DaemonErrorMessage error={error} part="remedy" /></section><section data-details><DaemonErrorMessage error={error} part="details" /></section></>);
  const remedy = container.querySelector("[data-remedy]")!;
  const details = container.querySelector("[data-details]")!;
  expect(hasErrorRemedy(error)).toBe(true);
  expect(remedy.textContent).not.toContain("/bin/zsh");
  expect(remedy.querySelector("p,div")).toBeNull();
  const [consequence, command] = [...details.querySelectorAll("p")];
  expect(consequence!.textContent).not.toContain("/bin/zsh");
  expect(command!.textContent).toContain("/bin/zsh -l -i -c 'env -0'");
  expect(details.textContent).not.toContain(remedy.textContent!);
});

it("keeps an ActionableError's detail as raw text of a block of its own whether or not its code is worded", () => {
  const error = new ActionableError("The file could not be read.", "path", { code: "path_not_found", params: { path: "/missing" }, detail: "ENOENT: no such file" });
  expect(hasErrorDetails(error)).toBe(true);
  const { body, raw } = mountApart(error);
  expect(body.textContent).toContain("/missing");
  expect(body.textContent).not.toContain("ENOENT");
  expect(raw.querySelector("pre")?.textContent).toBe("ENOENT: no such file");
});
