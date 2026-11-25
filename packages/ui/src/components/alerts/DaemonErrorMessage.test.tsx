// @vitest-environment jsdom
import { Description, Input, Label, TextField } from "@heroui/react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, onTestFinished } from "vitest";

import { ErrorAlerts } from "../../shell/ErrorAlerts";
import { errorAlerts } from "./errorAlerts";
import { raiseError } from "./raiseError";
import { CODES } from "../../errors/codes";
import { daemonMessage } from "../../errors/daemonMessage";
import { CodedError } from "../../errors/codedError";
import { droppedFilesParams, type DroppedFileEntry } from "../../errors/droppedFileEntries";
import { format, isMessageKey, translatorFor } from "../../i18n/catalog";
import { RecoveryGuidance, recoveryParams } from "./ErrorGuidance";
import { errorPresentation, hasCodeParams } from "../../errors/errorPresentation";
import { ActionableError } from "../../errors/actionableError";
import { useDialogAction } from "../dialog/dialogHooks";
import { LANGUAGES } from "../../i18n/languages";
import { currentLanguage, setLanguage } from "../../i18n/language";
import { announceDaemonHost } from "../../i18n/platformTerms";
import { createStateStore, DaemonProvider, type Daemon } from "../../store";
import { testDaemon } from "../../test/testDaemon";
import { TEST_PROVIDERS, UNPRESENTED_PROVIDER } from "../../test/testProviders";
import { DaemonErrorMessage } from "./DaemonErrorMessage";
import { hasErrorDetails, hasErrorRemedy, hasRawDetails } from "./errorPartPresence";
import { ErrorAlert } from "./ErrorAlert";
import { fakeTimers } from "../../test/clock";
import { ViewerHost } from "../../viewer/viewerTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const params = { shell: "/bin/zsh", command: "env -0", timeout: "10s" };
const failure = () => new CodedError("plain diagnostic for notifications", "shell_environment_timeout", params);

function mount(element: React.ReactNode): HTMLElement {
  const previousLanguage = currentLanguage();
  act(() => setLanguage("en"));
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => setLanguage(previousLanguage));
  });
  act(() => root.render(element));
  return container;
}

/** The whole message and, apart from it, its raw part, as the error alert dialog and a toast lay them out. */
function mountApart(error: CodedError, daemon: Daemon = testDaemon()): { body: Element; raw: Element } {
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
  act(() => setLanguage("zh-CN"));
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
  const unknown = new CodedError("new daemon error", "future_code", params);
  const incomplete = new CodedError("incomplete daemon error", "shell_environment_timeout", { shell: "/bin/zsh" });
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
  const error = new CodedError("plain", "conversation_to_resume_not_found", { agent: "Claude Code", path: "/Users/dev/.claude", id: "3f2b8c1e" });
  expect(errorPresentation(error)).toBe(error);
  const daemon = testDaemon();
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(Array.from(container.querySelectorAll("code"), (code) => code.textContent)).toEqual(["/Users/dev/.claude"]);
  // One element, so an alert's flex column cannot split the sentence into an item per literal.
  expect(container.children).toHaveLength(1);
  expect(container.textContent).toBe("No Claude Code conversation with this ID was found in /Users/dev/.claude.Check the ID, and that it was started under this account.");
});

it("sets resolved project and session names in chips without changing their typeface", () => {
  const daemon = testDaemon({ state: { sessions: new Map([["s1", { id: "s1", title: "Live title" } as import("../../protocol").Session]]) } });
  const error = new CodedError("plain", "conversation_in_session", { session: "s1" });
  expect(errorPresentation(error)).toBe(error);
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(container.querySelector("[data-inline-reference]")?.textContent).toBe("Live title");
  expect(container.querySelector("code")).toBeNull();
  expect(container.textContent).not.toMatch(/[“”]/);
  act(() => daemon.store.setState({ sessions: new Map([["s1", { id: "s1", title: "Renamed" } as import("../../protocol").Session]]) }));
  expect(container.querySelector("[data-inline-reference]")?.textContent).toBe("Renamed");
});

it("keeps the overlapping project name in a reference after the Trash remedy changes", () => {
  const error = new CodedError("plain", "project_directory_shared", { project: "Shared frontend" });
  const { body } = mountApart(error);
  expect(body.querySelector("[data-inline-reference]")?.textContent).toBe("Shared frontend");
  expect(body.textContent).toContain("The overlapping project must be removed");
  act(() => setLanguage("zh-CN"));
  expect(body.querySelector("[data-inline-reference]")?.textContent).toBe("Shared frontend");
  expect(body.textContent).toContain("需先移除重叠的项目");
});

it("keeps a failed launch's name and its nested failure's path as distinct chips", () => {
  const daemon = testDaemon();
  const params = { title: "Draft", reason_code: "path_not_found", path: "/missing/project" };
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={new CodedError("fallback", "launch_failed", params)} /></DaemonProvider>);
  expect(Array.from(container.querySelectorAll("[data-inline-reference]"), (chip) => chip.textContent)).toEqual(["Draft", "/missing/project"]);
  expect(container.querySelector("code")?.textContent).toBe("/missing/project");
  expect(container.textContent).toBe("Could not start Draft: /missing/project does not exist.");
});

it("renders structured running-session IDs as distinct live names, preserving commas and quote characters in titles", () => {
  const titles = ["Draft, review “notes”", "Second, “task”"];
  const daemon = testDaemon({ state: { sessions: new Map(titles.map((title, index) => [`s${index}`, { id: `s${index}`, title } as import("../../protocol").Session])) } });
  const params = { count: "3", sessions: "old fallback", session_ids: JSON.stringify(["s0", "s1", "gone"]) };
  const error = new CodedError("plain fallback", "session_has_running_sessions", params);
  expect(errorPresentation(error)).toBe(error);
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(Array.from(container.querySelectorAll("[data-inline-reference]"), (chip) => chip.textContent)).toEqual([...titles, "gone"]);
  expect(container.querySelector("code")?.textContent).toBe("gone");
  expect(container.textContent).not.toContain("old fallback");
  act(() => daemon.store.setState({ sessions: new Map([["s0", { id: "s0", title: "Renamed" } as import("../../protocol").Session]]) }));
  expect(Array.from(container.querySelectorAll("[data-inline-reference]"), (chip) => chip.textContent)).toEqual(["Renamed", "s1", "gone"]);
});

it("does not try to parse old or malformed running-session lists out of quoted title text", () => {
  const params = { count: "2", sessions: "“One, task”, “Other”" };
  for (const extra of [{}, { session_ids: "not JSON" }, { session_ids: '["s0",5]' }, { session_ids: "[]" }] as import("../../errors/codedError").MessageParams[]) {
    const error = new CodedError("old daemon fallback", "session_has_running_sessions", { ...params, ...extra });
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
  const error = new CodedError("plain fallback", "session_branches_not_removed", { count: "3", items: JSON.stringify(items) });
  const daemon = testDaemon({ state: { homeDir: "/home/dev" } });
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
  const daemon = testDaemon({ state: { sessions: new Map([["s1", { id: "s1", title: "Hub" } as import("../../protocol").Session]]) } });
  const reason = "account `Work`'s links to the default configuration could not be checked: permission denied";
  const empty = new CodedError(reason, "account_sharing_repair_failed", { account: "a1", name: "Work", items: "[]", cause: "permission_denied" });
  const unnamed = mountApart(empty, daemon);
  // Alone on its line it is a diagnostic block on its own surface, not a chip, and only in the raw part.
  expect(unnamed.raw.querySelector("pre code")?.textContent).toBe(reason);
  expect(unnamed.raw.querySelector("[data-inline-reference]")).toBeNull();
  expect(unnamed.body.textContent).toContain("could not be brought up to date");
  expect(unnamed.body.textContent).toContain("You do not have permission to change the account’s directory.");
  expect(unnamed.body.textContent).not.toContain(reason);
  const items = JSON.stringify([{ item: "skills/", left: "unchanged", detail: "permission denied", cause: "busy" }]);
  const listed = mountApart(new CodedError(reason, "account_sharing_repair_failed", { account: "a1", name: "Work", items }), daemon);
  expect(listed.body.textContent).toContain("skills/");
  expect(listed.body.textContent).toContain("is in use");
  expect(listed.body.textContent).not.toContain(reason);
  expect(listed.body.textContent).not.toContain("permission denied");
  expect(listed.raw.querySelector("li")?.lastElementChild?.textContent).toBe("permission denied");
  expect(listed.raw.textContent).toContain("skills/ was left unchanged.");
});

it("keeps the reason what goes with a removed project's folder is unknown as a code block apart from the sentence", () => {
  const error = new CodedError("plain fallback", "project_branches_not_determined", { name: "Docs", detail: "fatal: not a git repository" });
  const { body, raw } = mountApart(error);
  expect(raw.querySelector("pre")?.textContent).toBe("fatal: not a git repository");
  expect(body.textContent).not.toContain("fatal");
  expect(body.querySelector("pre")).toBeNull();
});

it("sets a worktree kept for a project folder that could not be checked apart, in the UI's own words, with what to do", () => {
  const items = [
    { kind: "worktree", name: "/home/dev/wt", branch: "a", state: "failed", uncommitted: false, detail: "whether it is or holds another project's folder could not be told", cause: "project_folder_unknown" },
    { kind: "branch", name: "fix/a", branch: "fix/a", state: "failed", uncommitted: false, detail: "error: refused", cause: null },
  ];
  const error = new CodedError("plain fallback", "session_branches_not_removed", { count: "2", items: JSON.stringify(items) });
  const daemon = testDaemon({ state: { homeDir: "/home/dev" } });
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} part="details" /></DaemonProvider>);
  expect(Array.from(container.querySelectorAll("p"), (line) => line.textContent)).toEqual([
    "Removing this failed:",
    "Whether this worktree is or holds another project's folder could not be checked:",
    "It may hold another project's files. Once it can be read, check what is in it before you remove it yourself.",
  ]);
  // The daemon's English is not shown for it; git's message for the other still is.
  expect(Array.from(container.querySelectorAll("pre"), (block) => block.textContent)).toEqual(["error: refused"]);
  expect(container.textContent).not.toContain("could not be told");
  expect(mountApart(error, daemon).raw.textContent).not.toContain("could not be told");
});

it("offers no Error details for a worktree kept for a project folder that could not be checked, when nothing raw is left", () => {
  const items = [
    { kind: "worktree", name: "/home/dev/wt", branch: "a", state: "failed", uncommitted: false, detail: "whether it is or holds another project's folder could not be told", cause: "project_folder_unknown" },
  ];
  const error = new CodedError("plain fallback", "session_branches_not_removed", { count: "1", items: JSON.stringify(items) });
  expect(hasRawDetails(error)).toBe(false);
});

it("leaves the line over the groups out when there is one group, whose lead says it all", () => {
  const items = [{ kind: "branch", name: "fix/a", branch: "fix/a", state: "unmerged", uncommitted: false, detail: null }];
  const error = new CodedError("plain fallback", "session_branches_not_removed", { count: "1", items: JSON.stringify(items) });
  const container = mount(<DaemonProvider value={testDaemon()}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(container.textContent).not.toContain("still there");
  expect(container.querySelector("p")?.textContent).toBe("This branch is not merged:");
});

// The split is a rendering boundary: every summary fits Alert.Description while the rest remains
// readable exactly once outside it, including a failure nested in another failure's sentence.
it.each([
  { name: "a simple structured reason", error: new CodedError("fallback", "path_not_found", { path: "/missing" }), summary: "/missing does not exist.", detail: undefined },
  { name: "multiline diagnostics", error: new CodedError("fallback", "project_branches_not_determined", { name: "Docs", detail: "fatal: one\nhint: two" }), summary: "Docs", detail: "fatal: one\nhint: two" },
  { name: "nested diagnostics", error: new CodedError("fallback", "launch_failed", { title: "Draft", reason_code: "project_branches_not_determined", name: "Docs", detail: "fatal: nested\nhint: two" }), summary: "Could not start Draft:", detail: "fatal: nested\nhint: two" },
  { name: "a shell timeout", error: failure(), summary: "A shell startup file is probably blocked", detail: "/bin/zsh -l -i -c 'env -0'" },
  { name: "a nested shell timeout", error: new CodedError("fallback", "launch_failed", { title: "Draft", reason_code: "shell_environment_timeout", ...params }), summary: "Could not start Draft: A shell startup file is probably blocked", detail: "/bin/zsh -l -i -c 'env -0'" },
  { name: "a branch group", error: new CodedError("fallback", "session_branches_not_removed", { count: "1", items: JSON.stringify([{ kind: "branch", name: "fix/a", branch: "fix/a", state: "failed", detail: "error: refused" }]) }), summary: "Removing this failed:", detail: "error: refused" },
])("keeps $name in inline summary and external details without duplication", ({ error, summary, detail }) => {
  const container = mount(<DaemonProvider value={testDaemon()}>
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
  const error = new CodedError("fallback", `unknown_${field}`, { [field]: "known" });
  const container = mount(<DaemonProvider value={testDaemon({ store })}><DaemonErrorMessage error={error} /></DaemonProvider>);
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
  const error = new CodedError("fallback", code, params);
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
  "provider.codex.unarchive_failed", "login_failed",
])("keeps %s diagnostics byte-for-byte once outside rich summaries and in plain text", (code) => {
  const detail = "fatal: /tmp/مراجعة/one\n    indented diagnostic\n" + "x".repeat(400);
  const params = { detail, agent: "Codex", name: "Docs" };
  const error = new CodedError("fallback", code, params);
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
  const error = new CodedError("fallback", "launch_failed", { title: "Draft", reason_code: "quota_command_failed", agent: "Codex", detail: "raw\n    quota failure" });
  const { body, raw } = mountApart(error);
  expect(body.querySelector("span")?.textContent).toBe("Could not start Draft: Codex could not report its usage.");
  expect(body.querySelector("pre")).toBeNull();
  expect(raw.querySelectorAll("pre")).toHaveLength(1);
  act(() => setLanguage("zh-CN"));
  expect(body.textContent).toContain("无法报告用量");
  expect(raw.querySelector("pre")?.textContent).toBe("raw\n    quota failure");
});

it.each(["unreadable_request", "project_directory_trash_failed", "configuration_unreadable"])("says %s in its own sentence, the system's text only in the raw part", (code) => {
  const detail = "Operation not permitted (os error 1)";
  const { body, raw } = mountApart(new CodedError("fallback", code, { detail }));
  expect(body.textContent).toBe(format("en", `daemon.${code}.summary` as never));
  expect(body.textContent).not.toContain(detail);
  expect(raw.querySelector("pre")?.textContent).toBe(detail);
});

it.each([
  ["unanswered", "the home folder did not respond in time, so whether this is it could not be told"],
  ["unreadable", "it could not be read, so whether another project's files are in it could not be told"],
])("says a project_directory_trash_failed with cause %s in its own words, with its own remedy, the daemon's text only in the raw part", (cause, detail) => {
  const error = new CodedError("fallback", "project_directory_trash_failed", { path: "/work/p", detail, cause });
  const { body, raw } = mountApart(error);
  const words = (language: "en" | "zh-CN") => ["summary", "recovery"].map((part) => format(language, `daemon.project_directory_trash_failed.${cause}.retryTrash.${part}` as never)).join("");
  expect(body.textContent).toBe(words("en"));
  expect(body.textContent).not.toContain(format("en", "daemon.project_directory_trash_failed.summary" as never));
  expect(raw.querySelector("pre")?.textContent).toBe(detail);
  expect(hasErrorRemedy(error)).toBe(true);
  act(() => setLanguage("zh-CN"));
  expect(body.textContent).toBe(words("zh-CN"));
});

it.each(Object.entries(CODES).filter(([, record]) => "noSummary" in record).map(([code]) => code))("gives %s no first line, its title saying what failed, and keeps the system's text only in the raw part", (code) => {
  const detail = "Operation not permitted (os error 1)";
  const error = new CodedError("fallback", code, { detail });
  const { body, raw } = mountApart(error);
  expect(isMessageKey(`daemon.${code}.summary`)).toBe(false);
  expect(mount(<DaemonErrorMessage error={error} part="summary" />).textContent).toBe("");
  expect(body.textContent).toBe("");
  expect(raw.querySelector("pre")?.textContent).toBe(detail);
  expect(daemonMessage("en", code, { detail }, "fallback", createStateStore({}).getState())).toContain(detail);
  expect(daemonMessage("en", code, { detail }, "fallback", createStateStore({}).getState(), true)).toBe("");
});

it.each(["git_restore_failed", "terminal_unavailable"])("shows %s with no detail as nothing of its own: no first line, no dangling colon, no Error details", (code) => {
  const error = new CodedError(`plain message for ${code}: `, code, { detail: "" });
  const { body, raw } = mountApart(error);
  expect(hasCodeParams(error)).toBe(true);
  expect(body.textContent).toBe("");
  expect(raw.textContent).toBe("");
  expect(hasErrorDetails(error)).toBe(false);
  expect(hasErrorRemedy(error)).toBe(false);
});

it("raises git_switch_failed as an alert titled for what failed, its body starting with the remedy and the text only under Error details", () => {
  const detail = "error: Your local changes would be overwritten by checkout";
  raiseError(new CodedError("fallback", "git_switch_failed", { detail }), { title: "Could not switch branches" });
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
  ["no_upstream_to_pull", {}, "This branch has no upstream to pull from.", "error.recovery.noUpstream"],
  ["not_on_a_branch", {}, "This project is not on a branch.", "error.recovery.branch"],
] as const)("raises %s as an alert titled for the failed pull, worded for a push and a pull alike, with its remedy", (code, codeParams, sentence, remedy) => {
  // A code with no parameters of its own has no structured layout: the alert shows the error's message, which the
  // connection words with `daemonMessage` as the failure arrives (`store/connection.ts`), so it is raised as it is there.
  const worded = daemonMessage("en", code, codeParams, "fallback", createStateStore({}).getState());
  expect(worded).toBe(sentence);
  raiseError(new CodedError(worded, code, codeParams), { title: "Could not pull the branch" });
  mount(<DaemonProvider value={testDaemon()}><ErrorAlerts /></DaemonProvider>);
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.querySelector("h2")?.textContent).toBe("Could not pull the branch");
  expect(alert.textContent).toContain(sentence);
  expect(alert.textContent).not.toContain("fallback");
  expect(alert.textContent).toContain(format("en", remedy, { terminal: "Terminal", git: "Git" }));
});

it("raises pull_merge_conflict with its sentence first, the remedy after it and the conflicted paths one per line under Error details", () => {
  const detail = "src/a.ts\nsrc/b.ts";
  raiseError(new CodedError("fallback", "pull_merge_conflict", { detail }), { title: "Could not pull the branch" });
  mount(<DaemonProvider value={testDaemon()}><ErrorAlerts /></DaemonProvider>);
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.querySelector("h2")?.textContent).toBe("Could not pull the branch");
  expect(alert.textContent).toContain("Merging the upstream into this branch causes conflicts, so the merge was cancelled and nothing changed.");
  expect(alert.textContent).not.toContain("fallback");
  const remedy = format("en", "error.recovery.mergeConflict", { terminal: "Terminal", git: "Git" });
  expect(alert.textContent).toContain(remedy);
  expect(alert.textContent!.indexOf("nothing changed.")).toBeLessThan(alert.textContent!.indexOf(remedy));
  expect(alert.querySelector("pre")?.textContent).toBe(detail);
});

it.each([
  { case: "a launch whose failure is internal", error: new CodedError("fallback", "launch_failed", { title: "Draft", reason_code: "internal_error", reason: "spawn failed: os error 2", detail: "spawn failed: os error 2" }), sentence: "Could not start Draft: Something unexpected went wrong.", rawText: "spawn failed: os error 2" },
  { case: "a launch whose failure the catalog lacks", error: new CodedError("fallback", "launch_failed", { title: "Draft", reason_code: "future_code", reason: "a newer daemon's reason" }), sentence: "Could not start Draft: Something unexpected went wrong.", rawText: "a newer daemon's reason" },
  { case: "a launch whose uncatalogued failure the system's text names", error: new CodedError("fallback", "launch_failed", { title: "Draft", reason_code: "future_code", reason: "No space left on device (os error 28)" }), sentence: "Could not start Draft: The disk is full.", rawText: "No space left on device (os error 28)" },
  { case: "a trust answer whose reason the catalog lacks", error: new CodedError("fallback", "trust_answer_failed", { agent: "Codex", reason_code: "future_reason", reason: "the daemon's English reason" }), sentence: "Octoboard could not press Codex's trust confirmation (something unexpected went wrong).", rawText: "the daemon's English reason" },
])("says $case with a generic reason, its own text only in the raw part", ({ error, sentence, rawText }) => {
  const { body, raw } = mountApart(error);
  expect(body.querySelector("span")?.textContent?.replace(/[\u2066-\u2069]/g, "")).toBe(sentence);
  expect(body.textContent).not.toContain(rawText);
  expect(raw.querySelector("pre")?.textContent).toBe(rawText);
  expect(hasErrorDetails(error)).toBe(true);
});

describe("a failure to write the application's own data nested in another failure", () => {
  const detail = "database or disk is full: Error code 13: Insertion failed because database is full";
  const nested = { reason_code: "store_write_failed", reason: `Octoboard could not read or write its own data: ${detail}`, cause: "disk_full", detail };

  it("says a failed launch with the summary of its cause, SQLite's text only in the raw part", () => {
    const error = new CodedError("fallback", "launch_failed", { title: "Draft", ...nested });
    const { body, raw } = mountApart(error);
    const text = body.textContent?.replace(/[\u2066-\u2069]/g, "");
    expect(text).toContain("Could not start Draft: The app could not read or write its own data.");
    expect(text).not.toContain("Error code 13");
    expect(raw.querySelector("pre")?.textContent).toBe(detail);
  });

  it("says a failed handoff archive by the receiving session, with the same summary", () => {
    const error = new CodedError("fallback", "handoff_archive_failed", { receiving_session: "s2", ...nested, cause: "other" });
    const { body, raw } = mountApart(error);
    const text = body.textContent?.replace(/[\u2066-\u2069]/g, "");
    expect(text).toContain("The work was handed off to s2, but archiving the original session failed: The app could not read or write its own data.");
    expect(text).not.toContain("Error code 13");
    expect(raw.querySelector("pre")?.textContent).toBe(detail);
  });

  it.each(["handoff_archive_failed", "handoff_move_failed"])("names the receiving session of %s by its title once the store holds it", (code) => {
    const daemon = testDaemon({ state: { sessions: new Map([["s2", { id: "s2", title: "Received work" } as import("../../protocol").Session]]) } });
    const error = new CodedError("fallback", code, { receiving_session: "s2", ...nested, cause: "other" });
    const { body } = mountApart(error, daemon);
    expect(body.querySelector("[data-inline-reference]")?.textContent).toBe("Received work");
    expect(body.textContent).not.toContain("s2");
  });

  it("says a failed move of the place by the receiving session, with the same summary", () => {
    const error = new CodedError("fallback", "handoff_move_failed", { receiving_session: "s2", ...nested, cause: "other" });
    const { body, raw } = mountApart(error);
    const text = body.textContent?.replace(/[\u2066-\u2069]/g, "");
    expect(text).toContain("The new session s2 was started, but handing the work off to it did not complete: The app could not read or write its own data.");
    expect(text).not.toContain("Error code 13");
    expect(raw.querySelector("pre")?.textContent).toBe(detail);
  });
});

describe("the alert of a handoff that failed after the work moved", () => {
  const archiveLine = "The original session is still open. Archive it yourself when you no longer need it.";
  const bothRunningLine = "Both the original session and the new one are running. Check which one holds the work, and archive the one you no longer need.";

  function alertText(code: string, reasonCode: string, reason = "raw reason"): string {
    raiseError(new CodedError("fallback", code, { receiving_session: "s2", session: "s1", reason, reason_code: reasonCode }), { title: "Could not hand off the session" });
    mount(<ErrorAlerts />);
    return document.querySelector('[role="alertdialog"]')!.textContent!;
  }

  it.each([
    { reasonCode: "session_locked", own: "Unlock it first.", remedy: "Open the original session’s action menu and choose" },
    { reasonCode: "session_lock_busy", own: "Try again when the operation finishes.", remedy: "Wait for that operation to finish." },
    { reasonCode: "internal_error", own: "", remedy: "" },
  ])("tells a failed archive nesting $reasonCode to archive the original, with no step to retry", ({ reasonCode, own, remedy }) => {
    const text = alertText("handoff_archive_failed", reasonCode);
    expect(text).toContain(archiveLine);
    if (own) expect(text).not.toContain(own);
    if (remedy) expect(text).toContain(remedy);
    expect(text).not.toMatch(/retry|try again|again/i);
    expect(text.split(archiveLine)).toHaveLength(2);
  });

  it.each(["session_locked", "session_lock_busy", "internal_error"])("tells a failed move nesting %s that both sessions are running, with no step to retry", (reasonCode) => {
    const text = alertText("handoff_move_failed", reasonCode);
    expect(text).toContain(bothRunningLine);
    expect(text).not.toContain(archiveLine);
    expect(text).not.toMatch(/retry|try again|again/i);
  });
});

describe("a failed automatic account switch", () => {
  const params = { agent: "Codex", from: "Work", account: "Spare", reason_code: "path_not_found", path: "/missing/project" };

  it("says what happened, then why on a line of its own with the nested failure's own chips", () => {
    const daemon = testDaemon();
    const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={new CodedError("fallback", "account_switch_automatically_failed", params)} /></DaemonProvider>);
    expect(Array.from(container.querySelectorAll("[data-inline-reference]:not(:has(code))"), (chip) => chip.textContent)).toEqual(expect.arrayContaining(["Work", "Codex", "Spare"]));
    expect(container.querySelector("code")?.textContent).toBe("/missing/project");
    const text = container.textContent?.replace(/[\u2066-\u2069]/g, "");
    expect(text).toContain("nor bring it back up on");
    expect(text).not.toMatch(/retry|try again/i);
    expect(text).toContain("/missing/project does not exist.");
  });

  it("leaves the nested failure's own remedy to the sentence's, which a failed launch's does not have", () => {
    const nested = { reason_code: "config_dir_unreachable", agent: "Codex", path: "/missing" };
    const switched = new CodedError("fallback", "account_switch_automatically_failed", { ...params, ...nested });
    const launched = new CodedError("fallback", "launch_failed", { title: "Draft", ...nested });
    expect(mount(<DaemonErrorMessage error={launched} />).textContent).toContain("Recreate it");
    const text = mount(<DaemonErrorMessage error={switched} />).textContent;
    expect(text).toContain("/missing");
    expect(text).not.toContain("Recreate it");
    expect(hasErrorRemedy(switched)).toBe(false);
    expect(hasErrorRemedy(launched)).toBe(true);
  });

  it("says a failed write to the application's own data as its summary, SQLite's text only in the raw part", () => {
    const detail = "database or disk is full: Error code 13: Insertion failed because database is full";
    const error = new CodedError("fallback", "account_switch_automatically_failed", { ...params, reason_code: "store_write_failed", reason: `Octoboard could not read or write its own data: ${detail}`, cause: "disk_full", detail });
    const { body, raw } = mountApart(error);
    expect(body.textContent?.replace(/[\u2066-\u2069]/g, "")).toContain("The app could not read or write its own data.");
    expect(body.textContent).not.toContain("Error code 13");
    expect(raw.querySelector("pre")?.textContent).toBe(detail);
  });

  it("says a reason the catalog lacks as the generic sentence, its own text only in the raw part", () => {
    const error = new CodedError("fallback", "account_switch_automatically_failed", { ...params, reason_code: "future_code", reason: "a newer daemon's reason" });
    const { body, raw } = mountApart(error);
    expect(body.textContent).toContain("Something unexpected went wrong.");
    expect(body.textContent).not.toContain("a newer daemon's reason");
    expect(raw.querySelector("pre")?.textContent).toBe("a newer daemon's reason");
    expect(hasErrorDetails(error)).toBe(true);
  });
});

it("uses name chips for fixed agent and product references while leaving plain messages usable", () => {
  const error = new CodedError("fallback", "agent_not_available", { agent: "Codex" });
  const daemon = testDaemon();
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(Array.from(container.querySelectorAll("[data-inline-reference]:not(:has(code))"), (chip) => chip.textContent)).toEqual(["Codex"]);
  // The supported agents are the providers the daemon serves, each named as the daemon's registry has it, listed one
  // to an item under the sentence that introduces them.
  const list = container.querySelector("[role=list]")!;
  expect(document.getElementById(list.getAttribute("aria-labelledby")!)?.textContent).toBe("To open a session, make sure one of these supported agents is on your PATH:");
  const labels = TEST_PROVIDERS.map((provider) => provider.label);
  expect(Array.from(list.querySelectorAll("[role=listitem] bdi"), (item) => item.textContent)).toEqual(labels);
  // Read as part of the alert's description, the names keep the language's separators between them.
  expect(Array.from(list.querySelectorAll("[role=listitem]"), (item) => item.textContent)).toEqual(labels.map((label, index) => (index < labels.length - 1 ? `${label}, ` : label)));
  // The remedy alone stays phrasing content, as the paragraphs and Alert descriptions that hold it require.
  const remedy = mount(<DaemonProvider value={daemon}><p data-remedy><DaemonErrorMessage error={error} part="remedy" /></p></DaemonProvider>).querySelector("[data-remedy]")!;
  expect(remedy.querySelectorAll("[role=listitem]")).toHaveLength(labels.length);
  expect(remedy.querySelector("div,p,ul,li")).toBeNull();
  const plain = daemonMessage("en", error.code, error.params, error.message, daemon.store.getState());
  expect(plain.replace(/[\u2066-\u2069]/g, "")).toContain(`make sure one of these supported agents is on your PATH: ${labels.join(", ")}.`);
});

it("names the agent of a refused operation by the label the daemon serves when it has no presentation module", () => {
  const error = new CodedError("fallback", "provider_unsupported", { agent: UNPRESENTED_PROVIDER.id, operation: "sign_in" });
  const daemon = testDaemon({ state: { providers: [...TEST_PROVIDERS, UNPRESENTED_PROVIDER] } });
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(container.textContent).toContain("Unpresented Agent");
  expect(container.textContent).not.toContain(UNPRESENTED_PROVIDER.id);
});

it.each([
  ["provider.codex.unarchive_failed.remedy", "Check that Codex runs in the login shell"],
  ["provider.grok.sandbox_unsupported.remedy", "continue that one in Grok Build itself"],
] as const)("words the remedy %s from its provider's module, naming the provider", (remedy, text) => {
  const container = mount(<DaemonProvider value={testDaemon()}><RecoveryGuidance recovery={remedy} /></DaemonProvider>);
  expect(container.textContent).toContain(text);
  expect(container.textContent).not.toMatch(/\{\w+\}/);
});

it.each([
  ["provider.codex.relocation_config_unreadable.remedy", ["config.toml"]],
  ["provider.codex.relocation_home_unlocatable.remedy", ["sqlite_home", "config.toml", "CODEX_SQLITE_HOME", "~", "HOME"]],
] as const)("renders provider-owned references in %s in every language", (remedy, references) => {
  const container = mount(<RecoveryGuidance recovery={remedy} />);
  for (const language of LANGUAGES) {
    act(() => setLanguage(language));
    expect(container.textContent).not.toMatch(/\{\w+\}/);
    const marked = [...container.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent);
    expect(marked).toHaveLength(references.length);
    expect(marked).toEqual(expect.arrayContaining([...references]));
  }
});

it("retains a named quota window as a rich name and a plain isolated value", () => {
  const params = { agent: "Codex", account: "Work", window: "模型 / Model weekly" };
  const container = mount(<DaemonErrorMessage error={new CodedError("fallback", "account_quota_exhausted", params)} />);
  expect(Array.from(container.querySelectorAll("[data-inline-reference]"), (chip) => chip.textContent)).toEqual(["Work", "Codex", params.window]);
  expect(daemonMessage("en", "account_quota_exhausted", params, "fallback", createStateStore({}).getState())).toContain(`\u2068${params.window}\u2069`);
});

it.each(["path_not_absolute", "project_directory_shared", "agent_not_available", "console_has_running_sessions", "trust_answer_failed", "conversation_in_session", "session_locked", "queued_messages_lost", "queued_messages_lost.held"])("keeps %s recovery visible as the remedy, with only its own further explanation for Details, and preserves the complete plain wording", (entry) => {
  // `queued_messages_lost.held` is that code with `fragment: "held"`, worded as a variant with further explanation of its own.
  const [code, variant] = entry.split(".");
  const params = { path: "/tmp/مراجعة", project: "Docs", agent: "Codex", reason_code: "screen_gone", session: "missing", reason: "fallback reason", ...(variant ? { fragment: variant } : {}) };
  const error = new CodedError("fallback", code, params);
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
  // A held fragment's Details never warn that the next message runs together with it.
  expect(details.textContent?.includes("run together")).toBe(further && !variant);
  const whole = () => normalize((summary.textContent ?? "") + details.textContent + remedy.textContent);
  // Plain text closes the supported agents' sentence with the joined list, which the rich remedy lists instead; with no
  // providers served that list is empty, leaving only the plain sentence's full stop.
  const plainOf = (text: string) => (code === "agent_not_available" ? text.replace(/[.。]$/, "") : text);
  expect(whole()).toBe(normalize(plainOf(plain)));
  act(() => setLanguage("zh-CN"));
  expect(whole()).toBe(normalize(plainOf(daemonMessage("zh-CN", code, params, error.message, createStateStore({}).getState()))));
});


it.each(["terminal_write_failed", "store_io_failed"])("keeps nested trust %s diagnostics outside the reason sentence", (reason_code) => {
  const detail = "Permission denied\n    /tmp/مراجعة";
  const error = new CodedError("fallback", "trust_answer_failed", { agent: "Codex", reason_code, detail });
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
  const error = new CodedError(fallback, "shell_environment_timeout", { detail });
  const { body, raw } = mountApart(error);
  expect(body.textContent).toBe("The shell environment could not be captured.");
  expect(raw.querySelector("pre")?.textContent).toBe(detail);
  expect(body.textContent).not.toMatch(/\{(?:shell|command|timeout)\}/);
  expect(daemonMessage("en", error.code, error.params, fallback, createStateStore({}).getState())).toBe(fallback);
});

it.each(["project", "console"])("preserves the held %s session's task and name as one raw directed chip", (owner) => {
  const session = { id: "held", title: "مراجعة name", terminal_title: "  Task / 任务  " } as import("../../protocol").Session;
  const store = createStateStore({ sessions: new Map([[session.id, session]]), projects: new Map([["owner", { id: "owner", name: "Owner" } as import("../../protocol").Project]]), consoles: new Map([["owner", { id: "owner", name: "Owner" } as import("../../protocol").Console]]) });
  const params = { session: session.id, [owner]: "owner" };
  const error = new CodedError("fallback", `conversation_held_in_${owner}`, params);
  const container = mount(<DaemonProvider value={testDaemon({ store })}><DaemonErrorMessage error={error} /></DaemonProvider>);
  const chips = [...container.querySelectorAll("[data-inline-reference]")];
  expect(chips.map((chip) => chip.textContent)).toEqual(["Task / 任务｜" + session.title, "Owner"]);
  expect(chips.every((chip) => chip.getAttribute("dir") === "auto" && !/[\u2066-\u2069]/.test(chip.textContent ?? ""))).toBe(true);
  // One chip around the joined title, each name isolated at element level so mixed directions
  // cannot reorder across the separator.
  expect([...chips[0].querySelectorAll("bdi")].map((name) => name.textContent)).toEqual(["Task / 任务", session.title]);
  for (const language of ["en", "zh-CN"] as const) {
    const plain = daemonMessage(language, error.code, params, error.message, store.getState());
    expect(plain).toContain("\u2068Task / 任务\u2069｜\u2068مراجعة name\u2069");
  }
  act(() => store.setState({ ...store.getState(), sessions: new Map([[session.id, { ...session, terminal_title: session.title }]]) }));
  expect([...container.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toEqual([session.title, "Owner"]);
});

it.each([
  ["tool", ["Git", "PATH"]],
  ["projectBranches", ["Terminal", "git worktree list", "git branch", "Git"]],
  ["sessionBranches", ["Terminal", "Git"]],
  ["repository", ["Git", "Git", "Terminal"]],
  ["noCommits", ["Terminal"]], ["shell", ["Terminal"]],
  ["gitFailure", ["Terminal", "Git", "Git"]], ["gitSwitchFailure", ["Terminal", "Git", "Git"]], ["gitRestoreFailure", ["Terminal", "Git"]], ["gitHistory", ["Terminal"]], ["noUpstream", ["Terminal"]], ["mergeConflict", ["Terminal"]],
  ["unsentMessage", ["Enter"]],
] as const)("marks %s recovery references while preserving a matching plain companion", (recovery, englishValues) => {
  const container = mount(<RecoveryGuidance recovery={recovery} />);
  for (const language of ["en", "zh-CN"] as const) {
    act(() => setLanguage(language));
    const chips = [...container.querySelectorAll("[data-inline-reference]")];
    const expected = language === "zh-CN" && recovery === "repository" ? ["Git", "终端", "Git"] : englishValues.map((value) => language === "zh-CN" && value === "Terminal" ? "终端" : value);
    expect(chips.map((chip) => chip.textContent)).toEqual(expected);
    for (const chip of chips) {
      const technical = ["PATH", "Enter", "git worktree list", "git branch"].includes(chip.textContent!);
      expect(chip.getAttribute("dir")).toBe(technical ? "ltr" : "auto");
      expect(!!chip.querySelector("code")).toBe(technical);
      expect(chip.textContent).not.toMatch(/[\u2066-\u2069]/);
    }
    const plain = format(language, `error.recovery.${recovery}`, recoveryParams(translatorFor(language)));
    expect(container.textContent).toBe(plain);
    expect(plain).not.toMatch(/\{\w+\}|<\/?(?:span|code)/);
  }
});

it.each(["accountSharing", "accountChanging"] as const)("names the sharing switch as a reference and the Settings page as a path in %s recovery, in the plain companion as text", (recovery) => {
  const container = mount(<RecoveryGuidance recovery={recovery} />);
  const names = { en: ["Share default configuration", "Settings", "Agent accounts"], "zh-CN": ["共享默认配置", "设置", "智能体账户"] } as const;
  for (const language of ["en", "zh-CN"] as const) {
    act(() => setLanguage(language));
    const [sharing, ...steps] = names[language];
    const path = container.querySelector("[data-inline-path]")!;
    expect([...path.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toEqual(steps);
    // Where the switch's name stands among them is the language's own order.
    expect([...container.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toHaveLength(3);
    expect([...container.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toEqual(expect.arrayContaining([sharing, ...steps]));
    const plain = format(language, `error.recovery.${recovery}`, recoveryParams(translatorFor(language)));
    expect(plain).toContain(steps.join(" > "));
    expect(plain).not.toMatch(/\{\w+\}/);
    expect(container.textContent).toBe(plain);
  }
});

it("keeps generic terminal nouns as prose rather than treating them as named controls", () => {
  const container = mount(<RecoveryGuidance recovery="stopProcess" />);
  expect(container.textContent).toBe("Open the affected session’s terminal and exit the agent using its quit command, then retry.");
  expect(container.querySelector("[data-inline-reference]")).toBeNull();
});

it("routes the operating-system reference in directory recovery through the shared rich/plain boundary", () => {
  const params = { path: "/Volumes/مراجعة", detail: "permission denied" };
  const container = mount(<DaemonErrorMessage error={new CodedError("fallback", "directory_unreadable", params)} />);
  const system = [...container.querySelectorAll("[data-inline-reference]")].find((chip) => chip.textContent === "macOS")!;
  expect(system.getAttribute("dir")).toBe("auto");
  expect(system.querySelector("code")).toBeNull();
  expect(system.closest("span.block")?.textContent).toContain("asks for permission per volume");
  // The raw text is Details' diagnostic block, and the visible sentence and remedy hold none of it.
  const summary = mount(<DaemonErrorMessage error={new CodedError("fallback", "directory_unreadable", params)} part="summary" />);
  const remedy = mount(<DaemonErrorMessage error={new CodedError("fallback", "directory_unreadable", params)} part="remedy" />);
  expect(summary.textContent + remedy.textContent).not.toContain("permission denied");
  expect(mount(<DaemonErrorMessage error={new CodedError("fallback", "directory_unreadable", params)} part="details" />).textContent).toContain("permission denied");
  const plain = daemonMessage("en", "directory_unreadable", params, "fallback", createStateStore({}).getState());
  expect(plain).toContain("\u2068macOS\u2069 asks for permission per volume");
  expect(plain).not.toMatch(/\{\w+\}/);
});

it("words an unreadable or unreachable directory by the daemon host's system, and anew when the host changes", () => {
  const params = { path: "/home/dev/code", detail: "permission denied" };
  const text = (code: string, part?: "summary" | "remedy" | "details") => mount(<DaemonErrorMessage error={new CodedError("fallback", code, params)} part={part} />).textContent;
  const plain = (code: string) => daemonMessage("en", code, params, "fallback", createStateStore({}).getState()).replace(/[\u2066-\u2069]/g, "");
  onTestFinished(() => act(() => announceDaemonHost(undefined)));
  // A macOS daemon host keeps the per-volume wording.
  act(() => announceDaemonHost({ os: "macos", terms: {} }));
  expect(text("directory_unreadable", "remedy")).toBe("Grant the application access to that volume, then try again.");
  expect(plain("directory_unreachable")).toContain("A packaged application needs file access granted per volume");
  // A Windows or Linux one says nothing of volumes, whichever system the client runs on.
  for (const os of ["windows", "linux"] as const) {
    act(() => announceDaemonHost({ os, terms: {} }));
    const name = os === "windows" ? "Windows" : "Linux";
    expect(text("directory_unreadable", "summary")).toBe("/home/dev/code could not be read.");
    expect(text("directory_unreadable", "remedy")).toBe("Check that the user account the daemon runs as can read the folder, then try again.");
    expect(text("directory_unreadable", "details")).toContain(`On ${name}, the daemon reads folders with the permissions of the user account it runs as.`);
    expect(text("directory_unreadable", "details")).not.toContain("per volume");
    expect(plain("directory_unreadable")).toBe(`/home/dev/code could not be read. On ${name}, the daemon reads folders with the permissions of the user account it runs as. Check that this account can read the folder, then try again: permission denied`);
    expect(plain("directory_unreachable")).toBe("/home/dev/code is not a directory the daemon can reach.");
  }
  // A message already shown is worded anew when the daemon host changes.
  act(() => announceDaemonHost({ os: "macos", terms: {} }));
  const shown = mount(<DaemonErrorMessage error={new CodedError("fallback", "directory_unreadable", params)} part="remedy" />);
  expect(shown.textContent).toBe("Grant the application access to that volume, then try again.");
  act(() => announceDaemonHost({ os: "windows", terms: {} }));
  expect(shown.textContent).toBe("Check that the user account the daemon runs as can read the folder, then try again.");
});

it("retains a named key inside a structured trust reason as a technical chip", () => {
  const params = { agent: "Codex", reason_code: "cursor_did_not_move" };
  const container = mount(<DaemonErrorMessage error={new CodedError("fallback", "trust_answer_failed", params)} />);
  expect(container.querySelector("code")?.textContent).toBe("Enter");
  expect(container.querySelector("code")?.getAttribute("dir")).toBe("ltr");
  expect(container.textContent).toContain("so Enter was not sent");
  expect(container.textContent).toContain("Answer it in the terminal.");
  const plain = daemonMessage("en", "trust_answer_failed", params, "fallback", createStateStore({}).getState());
  expect(plain).toContain("so \u2068Enter\u2069 was not sent");
});

it.each(["account_sharing_failed", "account_sharing_repair_failed"])("says %s short by cause at the top of the details part, ahead of the raw diagnostics", (code) => {
  const detail = "permission denied\n    /tmp/مراجعة/file\n";
  const session = { id: "s1", title: "مراجعة / Review, 名称" } as import("../../protocol").Session;
  const store = createStateStore({ sessions: new Map([[session.id, session]]) });
  const params = { name: "Work", account: "a1", detail: "do not repeat the daemon summary", items: JSON.stringify([
    { item: "skills/", left: "unchanged", detail, cause: "permission_denied" },
    { item: "plugins/", left: "unchanged", detail: "second", cause: "permission_denied" },
    { item: "conversation-id", left: "partly_moved", detail: "busy", sessions: ["s1", "gone"], cause: "busy" },
  ]) };
  const error = new CodedError("do not repeat the raw fallback", code, params);
  const container = mount(<DaemonProvider value={testDaemon({ store })}>
    <ErrorAlert title="Could not finish"><DaemonErrorMessage error={error} part="summary" /></ErrorAlert>
    <section><DaemonErrorMessage error={error} part="details" /></section>
  </DaemonProvider>);
  // The causes and their fixes are all further explanation: no main remedy of its own.
  expect(hasErrorRemedy(error)).toBe(false);
  const summary = container.querySelector('[data-slot="alert-description"]')!;
  const details = container.querySelector("section")!;
  for (const language of ["en", "zh-CN"] as const) {
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
  expect([...details.querySelectorAll("button")].map((button) => button.getAttribute("aria-label"))).toEqual(Array(3).fill(translatorFor("zh-CN")("common.copy")));
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
  const error = new CodedError("raw", "account_sharing_failed", { name: "Work", account: "a1", items: "[]", detail: "default dir moved", cause: "conflict" });
  const { body, raw } = mountApart(error);
  expect(body.textContent).toContain("The account’s directory conflicts with what sharing needs.");
  expect(body.textContent).not.toContain("default dir moved");
  expect(raw.querySelector("pre code")?.textContent).toBe("default dir moved");
  expect(raw.querySelector("[data-inline-reference]")).toBeNull();
});

it.each([["a cause the client does not know", "from_a_newer_daemon"], ["no cause", undefined]])("words a whole sharing failure with %s as other: no reason line, the details pointed at", (_name, cause) => {
  const mountWith = (detail: string | undefined) => mount(<DaemonProvider value={testDaemon()}>
    <DaemonErrorMessage error={new CodedError("raw", "account_sharing_failed", { name: "Work", account: "a1", items: "[]", detail, cause } as never)} />
  </DaemonProvider>);
  expect(mountWith("x").querySelector("span.block")!.textContent).toBe("See the details.");
  expect(mountWith(undefined).querySelector("span.block")).toBeNull();
});

it("joins a whole failure's reason and its guidance in the catalog, and points a cause with no line of its own at Details", () => {
  const mountWith = (cause: string, detail: string | undefined) => mount(<DaemonProvider value={testDaemon()}>
    <DaemonErrorMessage error={new CodedError("raw", "account_sharing_failed", { name: "Work", account: "a1", items: "[]", detail, cause } as never)} />
  </DaemonProvider>);
  const busy = mountWith("busy", "x");
  const other = mountWith("other", "x");
  act(() => setLanguage("zh-CN"));
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
  const error = new CodedError(original, code, { name: "Work", account: "a1", items, detail });
  const container = mount(<><section data-summary><DaemonErrorMessage error={error} part="summary" /></section><section data-details><DaemonErrorMessage error={error} part="details" /></section></>);
  expect(container.querySelector("[data-summary]")?.textContent).toContain("Work");
  expect(container.querySelector("[data-summary]")?.textContent).not.toContain(diagnostic);
  expect(container.querySelector("[data-summary] pre")).toBeNull();
  expect(container.querySelectorAll("pre")).toHaveLength(1);
  expect(container.querySelector("[data-details] pre")?.textContent).toBe(diagnostic);
  expect(container.querySelector("pre")?.getAttribute("dir")).toBe("ltr");
});

it("shows a session left interrupted after a sharing change as a sentence with the reason as a code block", () => {
  const error = new CodedError("raw", "sharing_resume_failed", { account: "a1", name: "Work", detail: "no such file" });
  const { body, raw } = mountApart(error);
  expect(body.textContent).toContain("could not be resumed after its sharing changed");
  expect(body.querySelector("[data-inline-reference]")?.textContent).toBe("Work");
  expect(body.textContent).not.toContain("no such file");
  expect(raw.querySelector("pre")?.textContent).toBe("no such file");
});

it("shows the still-running variant of the resume notice without 'left interrupted', keeping the reason as a code block", () => {
  const error = new CodedError("raw", "sharing_resume_failed", { account: "a1", name: "Work", detail: "timed out", still_running: "true" });
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
  const error = new CodedError("raw", code, failureParams as never);
  const container = mount(<DaemonProvider value={testDaemon()}><DaemonErrorMessage error={error} part="details" /></DaemonProvider>);
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
  const error = new CodedError("raw", code, failureParams as never);
  const container = mount(<DaemonProvider value={testDaemon()}><DaemonErrorMessage error={error} part="remedy" /></DaemonProvider>);
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

describe("a failure that states only its cause", () => {
  const trashFailed = (detail: string, extra: Record<string, string> = {}) => new CodedError("plain", "account_directory_trash_failed", { path: "/a", detail, ...extra });

  it("says the cause the system's text names after its sentence, with no remedy", () => {
    const { body, raw } = mountApart(trashFailed("Operation not permitted (os error 1)"));
    expect(body.textContent).toBe("The directory could not be moved to the Trash. Access was denied.");
    expect(raw.textContent).toContain("Operation not permitted");
  });

  it("says no cause where the system's text names none, or where the sentence is the cause", () => {
    expect(mountApart(trashFailed("something else")).body.textContent).toBe("The directory could not be moved to the Trash.");
  });
});

describe("a draft cleared from an input box", () => {
  const CHINESE = "远程的菜单栏图标中";
  const label = (body: Element, text: string) => [...body.querySelectorAll("span[id]")].find((span) => span.textContent === text);

  it.each([CHINESE, "مسودة لم تكتمل\nsecond line"])("shows the draft %j whole in the body, under its label, in its own direction, wrapped, with nothing raw", (draft) => {
    const error = new CodedError("plain", "draft_discarded", { agent: "Claude Code", detail: draft });
    const { body, raw } = mountApart(error);
    expect(hasErrorDetails(error)).toBe(true);
    expect(hasRawDetails(error)).toBe(false);
    expect(raw.textContent).toBe("");
    const block = body.querySelector("pre")!;
    expect(block.textContent).toBe(draft);
    expect(block.getAttribute("dir")).toBe("auto");
    // Its long lines wrap, unlike a diagnostic's; the copy button still copies the text as it is.
    expect(block.classList.contains("whitespace-pre-wrap")).toBe(true);
    const group = label(body, "Draft content")?.parentElement;
    expect(group?.contains(block)).toBe(true);
    expect(group?.querySelector("button")).not.toBeNull();
    expect(body.textContent).not.toContain("Error details");
  });

  it("describes its alert by the explanation alone, the draft being a block of its own after it", () => {
    raiseError(new CodedError("plain", "draft_discarded", { agent: "Claude Code", detail: CHINESE }), { title: "Draft" });
    mount(<ErrorAlerts />);
    const alert = document.querySelector('[role="alertdialog"]')!;
    const description = document.getElementById(alert.getAttribute("aria-describedby")!)!;
    expect(description.textContent).toContain("cleared the draft");
    expect(description.textContent).toContain("copy it below");
    expect(description.textContent).not.toContain(CHINESE);
    const block = alert.querySelector("pre")!;
    expect(block.textContent).toBe(CHINESE);
    expect(description.contains(block)).toBe(false);
    // The draft comes after the explanation, not before it.
    expect(description.compareDocumentPosition(block) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(alert.querySelectorAll("pre")).toHaveLength(1);
    expect(alert.textContent).not.toContain("Error details");
  });

  it("describes an alert with no draft attached by its whole body", () => {
    raiseError(new CodedError("plain", "session_locked", {}), { title: "Locked" });
    mount(<ErrorAlerts />);
    const alert = document.querySelector('[role="alertdialog"]')!;
    const description = document.getElementById(alert.getAttribute("aria-describedby")!)!;
    const prose = alert.querySelector<HTMLElement>(".whitespace-pre-wrap")!;
    expect(prose.textContent).not.toBe("");
    expect(description.contains(prose)).toBe(true);
    expect(description.textContent).toContain(prose.textContent);
  });

  it("names the text, not error details, when copying the draft fails", async () => {
    const error = new CodedError("plain", "draft_discarded", { agent: "Claude Code", detail: CHINESE });
    const failures: string[] = [];
    fakeTimers();
    const original = document.execCommand;
    Object.assign(document, { execCommand: () => false });
    onTestFinished(() => void Object.assign(document, { execCommand: original }));
    const container = mount(
      <ViewerHost clipboard={{ writeText: async () => Promise.reject(new Error("denied")) }} notifications={{ notice: () => {}, error: (message) => void failures.push(message) }}>
        <DaemonErrorMessage error={error} />
      </ViewerHost>,
    );
    act(() => {
      const button = container.querySelector<HTMLButtonElement>('button[aria-label="Copy"]')!;
      for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) button.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
    });
    await act(async () => {});
    expect(failures).toEqual(["The clipboard could not be written to."]);
    expect(errorAlerts.getSnapshot()[0]).toMatchObject({ title: "Could not copy the text" });
  });

  it("says when only part of the draft was cleared and that the message waits, under a title of its own", () => {
    const error = new CodedError("plain", "draft_discarded", { agent: "Claude Code", detail: CHINESE, cleared: "part" });
    const { body } = mountApart(error);
    expect(body.textContent).toContain("cleared part of the draft");
    expect(body.textContent).toContain("waits until the input box is empty");
    expect(body.textContent).toContain("part of it may still be in the input box");
    expect(label(body, "Draft content")?.parentElement?.contains(body.querySelector("pre"))).toBe(true);
    expect(body.querySelector("pre")!.textContent).toBe(CHINESE);
    raiseError(error, { title: "Draft" });
    mount(<ErrorAlerts />);
    const alert = document.querySelector('[role="alertdialog"]')!;
    expect(alert.querySelector("h2")?.textContent).toBe("Draft partly cleared");
  });

  it("puts the draft on a line of its own after the message as plain text", () => {
    const text = daemonMessage("en", "draft_discarded", { agent: "Claude Code", detail: CHINESE }, "fallback", createStateStore({}).getState());
    expect(text.endsWith(`\n${CHINESE}`)).toBe(true);
    expect(text).not.toContain("{detail}");
  });

  it("puts the draft of a partial clear on a line of its own after the message as plain text", () => {
    const text = daemonMessage("en", "draft_discarded", { agent: "Claude Code", detail: CHINESE, cleared: "part" }, "fallback", createStateStore({}).getState());
    expect(text).toContain("cleared part of the draft");
    expect(text.endsWith(`\n${CHINESE}`)).toBe(true);
  });

  it("words the label in the language in use", () => {
    const error = new CodedError("plain", "draft_discarded", { agent: "Claude Code", detail: CHINESE });
    const { body } = mountApart(error);
    act(() => setLanguage("zh-CN"));
    const localized = format("zh-CN", "daemon.draft_discarded.detailLabel");
    expect(localized).not.toBe(format("en", "daemon.draft_discarded.detailLabel"));
    expect(label(body, localized)?.parentElement?.contains(body.querySelector("pre"))).toBe(true);
    expect(label(body, "Draft content")).toBeUndefined();
    expect(body.querySelector("pre")!.textContent).toBe(CHINESE);
  });

  it("shows no block of its own where the notice carries no draft", () => {
    const error = new CodedError("plain", "draft_discarded", { agent: "Claude Code" });
    const { body } = mountApart(error);
    expect(body.querySelector("pre")).toBeNull();
    expect(label(body, "Draft content")).toBeUndefined();
  });
});

describe("a message the agent gave back at a cancel, cleared from an input box", () => {
  const MESSAGE = "Instruction from your parent session\nرسالة second line";
  const label = (body: Element, text: string) => [...body.querySelectorAll("span[id]")].find((span) => span.textContent === text);
  const cleared = (params: Record<string, string> = {}) => new CodedError("plain", "message_cleared_after_cancel", { agent: "Claude Code", detail: MESSAGE, ...params });

  it("says the message was not carried out, never that a draft was cleared, with the message whole under its own label", () => {
    const error = cleared();
    const { body, raw } = mountApart(error);
    expect(hasErrorDetails(error)).toBe(true);
    expect(hasRawDetails(error)).toBe(false);
    expect(raw.textContent).toBe("");
    expect(body.textContent).toContain("Claude Code");
    expect(body.textContent).toContain("was not carried out");
    expect(body.textContent).toContain("send it again");
    expect(body.textContent).not.toContain("draft");
    const block = body.querySelector("pre")!;
    expect(block.textContent).toBe(MESSAGE);
    expect(block.getAttribute("dir")).toBe("auto");
    expect(label(body, "Message content")?.parentElement?.contains(block)).toBe(true);
    expect(label(body, "Draft content")).toBeUndefined();
    expect(body.textContent).not.toContain("Error details");
  });

  it("titles its alert for the message, and describes it by the explanation alone", () => {
    raiseError(cleared(), { title: "Message" });
    mount(<ErrorAlerts />);
    const alert = document.querySelector('[role="alertdialog"]')!;
    expect(alert.querySelector("h2")?.textContent).toBe("Message not carried out");
    const description = document.getElementById(alert.getAttribute("aria-describedby")!)!;
    expect(description.textContent).toContain("was not carried out");
    expect(description.textContent).not.toContain(MESSAGE);
    expect(alert.querySelector("pre")!.textContent).toBe(MESSAGE);
  });

  it("says when only part of the message was cleared, under a title that still says what did not get done, and nothing of what became of the next message", () => {
    const error = cleared({ cleared: "part" });
    const { body } = mountApart(error);
    expect(body.textContent).toContain("cleared part of it");
    expect(body.textContent).not.toContain("waits");
    expect(body.textContent).toContain("part of it may still be in the input box");
    expect(body.querySelector("pre")!.textContent).toBe(MESSAGE);
    raiseError(error, { title: "Message" });
    mount(<ErrorAlerts />);
    expect(document.querySelector('[role="alertdialog"]')!.querySelector("h2")?.textContent).toBe("Message not carried out");
  });

  it.each<Record<string, string>>([{}, { cleared: "part" }])("puts the message on a line of its own after the text as plain text, with every placeholder filled (%j)", (params) => {
    const text = daemonMessage("en", "message_cleared_after_cancel", { agent: "Claude Code", detail: MESSAGE, ...params }, "fallback", createStateStore({}).getState());
    expect(text.endsWith(`\n${MESSAGE}`)).toBe(true);
    expect(text).not.toMatch(/\{[a-zA-Z_]+\}/);
    expect(text).toContain("Claude Code");
  });

  it("words the label and the title in the language in use", () => {
    const { body } = mountApart(cleared());
    act(() => setLanguage("zh-CN"));
    const localized = format("zh-CN", "daemon.message_cleared_after_cancel.detailLabel");
    expect(localized).not.toBe(format("en", "daemon.message_cleared_after_cancel.detailLabel"));
    expect(label(body, localized)?.parentElement?.contains(body.querySelector("pre"))).toBe(true);
    expect(format("zh-CN", "daemon.message_cleared_after_cancel.alertTitle")).not.toBe(format("en", "daemon.message_cleared_after_cancel.alertTitle"));
  });
});

describe("a message the agent took through its own server or channel and gave back", () => {
  const MESSAGE = "Instruction from your parent session\nرسالة second line";
  const givenBack = () => new CodedError("plain", "message_given_back_unsent", { agent: "Codex", detail: MESSAGE });

  it("says the agent gave the message back and it was not carried out, never that an input box held it or was cleared", () => {
    const error = givenBack();
    const { body, raw } = mountApart(error);
    expect(hasErrorDetails(error)).toBe(true);
    expect(hasRawDetails(error)).toBe(false);
    expect(raw.textContent).toBe("");
    expect(body.textContent).toContain("Codex");
    expect(body.textContent).toContain("gave it back without acting on it");
    // "Unsent" belongs to a message left in the input box, which this one never was.
    expect(body.textContent).not.toContain("unsent");
    expect(body.textContent).toContain("was not carried out");
    expect(body.textContent).toContain("send it again");
    expect(body.textContent).not.toContain("input box");
    expect(body.textContent).not.toContain("cleared");
    const block = body.querySelector("pre")!;
    expect(block.textContent).toBe(MESSAGE);
    expect(block.getAttribute("dir")).toBe("auto");
    expect([...body.querySelectorAll("span[id]")].find((span) => span.textContent === "Message content")?.parentElement?.contains(block)).toBe(true);
  });

  it("titles its alert for the message, and puts the message after the text as plain text with every placeholder filled", () => {
    raiseError(givenBack(), { title: "Message" });
    mount(<ErrorAlerts />);
    expect(document.querySelector('[role="alertdialog"]')!.querySelector("h2")?.textContent).toBe("Message not carried out");
    const text = daemonMessage("en", "message_given_back_unsent", { agent: "Codex", detail: MESSAGE }, "fallback", createStateStore({}).getState());
    expect(text.endsWith(`\n${MESSAGE}`)).toBe(true);
    expect(text).not.toMatch(/\{[a-zA-Z_]+\}/);
  });
});

describe("a message the agent held and dropped when its turn was cancelled", () => {
  const MESSAGE = "Instruction from your parent session\nرسالة second line";
  const dropped = () => new CodedError("plain", "message_dropped_at_cancel", { agent: "Codex", detail: MESSAGE });

  it("says the agent dropped the message and it was not carried out, never that it was given back, held in an input box or cleared", () => {
    const error = dropped();
    const { body, raw } = mountApart(error);
    expect(hasErrorDetails(error)).toBe(true);
    expect(hasRawDetails(error)).toBe(false);
    expect(raw.textContent).toBe("");
    expect(body.textContent).toContain("Codex");
    expect(body.textContent).toContain("was holding a message");
    expect(body.textContent).toContain("dropped it when it was interrupted");
    expect(body.textContent).not.toContain("unsent");
    expect(body.textContent).toContain("was not carried out");
    expect(body.textContent).toContain("send it again");
    expect(body.textContent).not.toContain("gave it back");
    expect(body.textContent).not.toContain("input box");
    expect(body.textContent).not.toContain("cleared");
    const block = body.querySelector("pre")!;
    expect(block.textContent).toBe(MESSAGE);
    expect(block.getAttribute("dir")).toBe("auto");
    expect([...body.querySelectorAll("span[id]")].find((span) => span.textContent === "Message content")?.parentElement?.contains(block)).toBe(true);
  });

  it("titles its alert for the message, and puts the message after the text as plain text with every placeholder filled", () => {
    raiseError(dropped(), { title: "Message" });
    mount(<ErrorAlerts />);
    expect(document.querySelector('[role="alertdialog"]')!.querySelector("h2")?.textContent).toBe("Message not carried out");
    const text = daemonMessage("en", "message_dropped_at_cancel", { agent: "Codex", detail: MESSAGE }, "fallback", createStateStore({}).getState());
    expect(text.endsWith(`\n${MESSAGE}`)).toBe(true);
    expect(text).not.toMatch(/\{[a-zA-Z_]+\}/);
  });

  it("is worded apart from a message given back, in every language", () => {
    for (const language of LANGUAGES) {
      expect(format(language, "daemon.message_dropped_at_cancel.summary"), language).not.toBe(format(language, "daemon.message_given_back_unsent.summary"));
    }
  });
});

describe("a message sent with the text in an input box that could not be cleared", () => {
  const CHINESE = "远程的菜单栏图标中";
  const label = (body: Element, text: string) => [...body.querySelectorAll("span[id]")].find((span) => span.textContent === text);
  const alertOf = (error: CodedError) => {
    raiseError(error, { title: "Message" });
    mount(<ErrorAlerts />);
    return document.querySelector('[role="alertdialog"]')!;
  };

  it.each([
    ["the draft", { agent: "Codex", detail: CHINESE }, "sent together with the draft", "Draft content"],
    ["the previous message and the text typed after it", { agent: "Codex", detail: CHINESE, held: "message_and_draft" }, "with text typed after it", "Input box content"],
  ])("hands %s back whole as a block of its own after the explanation, under its label, with a copy button", (_, params, wording, blockLabel) => {
    const error = new CodedError("plain", "message_sent_with_draft", params);
    const { body, raw } = mountApart(error);
    expect(hasErrorDetails(error)).toBe(true);
    expect(hasRawDetails(error)).toBe(false);
    expect(raw.textContent).toBe("");
    expect(body.textContent).toContain(wording);
    expect(body.textContent).toContain("copy it below");
    const block = body.querySelector("pre")!;
    expect(block.textContent).toBe(CHINESE);
    expect(block.getAttribute("dir")).toBe("auto");
    expect(block.classList.contains("whitespace-pre-wrap")).toBe(true);
    const group = label(body, blockLabel)?.parentElement;
    expect(group?.contains(block)).toBe(true);
    expect(group?.querySelector("button")).not.toBeNull();
    expect(body.textContent).not.toContain("Error details");
    // Describes its alert by the explanation alone: the text is the block that follows it.
    const alert = alertOf(error);
    const description = document.getElementById(alert.getAttribute("aria-describedby")!)!;
    expect(description.textContent).not.toContain(CHINESE);
    expect(description.contains(alert.querySelector("pre"))).toBe(false);
    expect(alert.querySelector("pre")!.textContent).toBe(CHINESE);
  });

  it.each([
    ["with the draft, under its own title", { agent: "Codex", detail: CHINESE }, "Message sent with the draft"],
    ["with the previous message, under a title of its own", { agent: "Codex", held: "message" }, "Message sent with the previous one"],
    ["with the previous message and the text typed after it, under a title of its own", { agent: "Codex", detail: CHINESE, held: "message_and_draft" }, "Message sent with the previous one and the draft"],
    ["with an input box that could not be read, under a title of its own", { agent: "Codex" }, "Message sent with the input box's content"],
  ])("titles the alert of a message sent %s", (_, params, title) => {
    const alert = alertOf(new CodedError("plain", "message_sent_with_draft", params));
    expect(alert.querySelector("h2")?.textContent).toBe(title);
  });

  it("says that the previous message was in the box, with no block, where no text could be handed back", () => {
    const error = new CodedError("plain", "message_sent_with_draft", { agent: "Codex", held: "message" });
    const { body } = mountApart(error);
    expect(body.textContent).toContain("previous message");
    expect(body.textContent).toContain("still unsent");
    expect(body.querySelector("pre")).toBeNull();
    expect(label(body, "Draft content")).toBeUndefined();
  });

  it("says that the box held whatever it held, with no block, where the box could not be read", () => {
    const error = new CodedError("plain", "message_sent_with_draft", { agent: "Codex" });
    const { body } = mountApart(error);
    expect(body.textContent).toContain("whatever the box held");
    expect(body.querySelector("pre")).toBeNull();
    expect(label(body, "Draft content")).toBeUndefined();
    const alert = alertOf(error);
    expect(alert.querySelector("h2")?.textContent).not.toBe("");
    expect(alert.querySelector("pre")).toBeNull();
  });

  it("puts the text on a line of its own after the message as plain text, for each variant that carries it", () => {
    const state = createStateStore({}).getState();
    for (const params of [{ agent: "Codex", detail: CHINESE }, { agent: "Codex", detail: CHINESE, held: "message_and_draft" }] as Record<string, string>[]) {
      const text = daemonMessage("en", "message_sent_with_draft", params, "fallback", state);
      expect(text.endsWith(`\n${CHINESE}`)).toBe(true);
      expect(text).not.toMatch(/\{[a-zA-Z_]+\}/);
    }
    for (const params of [{ agent: "Codex" }, { agent: "Codex", held: "message" }] as Record<string, string>[]) {
      const text = daemonMessage("en", "message_sent_with_draft", params, "fallback", state);
      expect(text).not.toContain(CHINESE);
      expect(text).not.toMatch(/\{[a-zA-Z_]+\}/);
      expect(text.trim()).not.toBe("");
    }
  });

  it("words the label in the language in use", () => {
    const error = new CodedError("plain", "message_sent_with_draft", { agent: "Codex", detail: CHINESE });
    const { body } = mountApart(error);
    act(() => setLanguage("zh-CN"));
    const localized = format("zh-CN", "daemon.message_sent_with_draft.detailLabel");
    expect(localized).not.toBe("Draft content");
    expect(label(body, localized)?.parentElement?.contains(body.querySelector("pre"))).toBe(true);
    expect(label(body, "Draft content")).toBeUndefined();
    expect(body.querySelector("pre")!.textContent).toBe(CHINESE);
  });

  it("tells that a message waits on an input box that cannot be read, under its own title, with the way out", () => {
    const error = new CodedError("plain", "message_waits_on_input", { agent: "Claude Code" });
    const { body } = mountApart(error);
    expect(body.textContent).toContain("waiting because this session's input box cannot be read");
    expect(body.textContent).toContain("send or clear what the input box holds");
    expect(body.querySelector("pre")).toBeNull();
    const alert = alertOf(error);
    expect(alert.querySelector("h2")?.textContent).toBe("Message waiting to go in");
    expect(body.textContent).toContain("Close the list or view");
  });
});

it("no longer says that an unconfirmed draft save may have sent the message with the draft", () => {
  const error = new CodedError("plain", "draft_save_unconfirmed", { agent: "Claude Code" });
  const { body } = mountApart(error);
  const text = body.textContent ?? "";
  expect(text).toContain("did not write its message");
  expect(text).not.toMatch(/may have been sent|may include|with the draft|unfinished/i);
});

it("splits an agent app that could not be opened into its sentence, what to do, and the daemon's reason behind Details", () => {
  const params = { detail: "the Codex application is not installed" };
  const part = (name: "summary" | "remedy" | "details") => mount(<DaemonErrorMessage error={new CodedError("fallback", "native_app_unavailable", params)} part={name} />).textContent;

  expect(part("summary")).toBe("It may not be installed, may not have started, or has no conversation to open.");
  expect(part("remedy")).toBe("See the details for the cause, then try again.");
  expect(part("details")).toContain("the Codex application is not installed");
  expect((part("summary") ?? "") + (part("remedy") ?? "")).not.toContain("Codex application");
  // Plain text (a toast, a notification) joins the sentence and the step, and carries none of the raw reason.
  const plain = daemonMessage("en", "native_app_unavailable", params, "fallback", createStateStore({}).getState());
  expect(plain).toBe("The agent's app may not be installed, may not have started, or has no conversation to open. See the details for the cause, then try again.");
});

describe("the files of a drop that were refused", () => {
  const entries: DroppedFileEntry[] = [
    { name: "docs", code: "dropped_file_folder", params: { name: "docs" }, message: "docs is a folder" },
    { name: "big.zip", code: "dropped_file_too_large", params: { name: "big.zip", limit_mib: "3" }, message: "big.zip is too large" },
    { name: "notes, final.txt", code: "session_not_running", params: {}, message: "This session is not running." },
  ];
  const refused = () => new CodedError("fallback", "dropped_files_refused", droppedFilesParams(entries));
  const plain = (node: Element | null | undefined) => node?.textContent?.replace(/[\u2066-\u2069]/g, "");
  const mountPart = (part?: "summary" | "details" | "remedy" | "raw" | "own") =>
    mount(<DaemonProvider value={testDaemon()}><DaemonErrorMessage error={refused()} part={part} /></DaemonProvider>);

  it("is a lead line and one row to a file", () => {
    const container = mountPart();
    expect(plain(container.querySelector("span"))).toBe("These files were not attached, each for its own reason:");
    expect(Array.from(container.querySelectorAll("li"), plain)).toEqual([
      "docs is a folder, and only files can be attached.",
      "big.zip is larger than 3 MiB.",
      "notes, final.txt could not be attached. This session is not running.",
    ]);
  });

  it("names each file as a reference", () => {
    const container = mountPart();
    expect(Array.from(container.querySelectorAll("li"), (row) => row.querySelector("[data-inline-reference]")?.textContent)).toEqual(["docs", "big.zip", "notes, final.txt"]);
  });

  it("gives the lead line alone as the summary and the list alone as the details", () => {
    const summary = mountPart("summary");
    expect(plain(summary)).toBe("These files were not attached, each for its own reason:");
    expect(summary.querySelector("li")).toBeNull();
    const details = mountPart("details");
    expect(details.querySelectorAll("li")).toHaveLength(3);
    expect(plain(details)).not.toContain("These files were not attached, each for its own reason:");
  });

  it.each(["remedy", "raw", "own"] as const)("has nothing as its %s part", (part) => {
    expect(mountPart(part).textContent).toBe("");
  });

  it("has details to show", () => {
    expect(hasErrorDetails(refused())).toBe(true);
  });
});
