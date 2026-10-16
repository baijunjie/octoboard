// @vitest-environment jsdom
import { Description, Input, Label, TextField } from "@heroui/react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { errorPresentation } from "../errorPresentation";
import { DialogError, useDialogAction } from "../dialogs/Dialog";
import { isolate } from "../i18n/isolate";
import { currentLanguage, setLanguage } from "../i18n/language";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { DaemonErrorMessage } from "./DaemonErrorMessage";

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
    act(() => setLanguage(previousLanguage));
  });
  act(() => root.render(element));
  return container;
}

it("retains the structured request failure through the dialog action and responds to language changes", async () => {
  let run!: (action: () => Promise<void>) => Promise<void>;
  function Subject() {
    const action = useDialogAction();
    run = action.run;
    return <DialogError message={action.error} />;
  }
  const container = mount(<Subject />);
  const error = failure();
  await act(() => run(() => Promise.reject(error)));
  const alert = container.querySelector('[role="alert"]');
  expect(alert?.closest('[data-slot="alert-root"]')?.classList.contains("alert--danger")).toBe(true);
  expect(container.querySelectorAll('[role="alert"]')).toHaveLength(1);
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

it("uses the plain fallback for an unfamiliar code or an incomplete shell timeout", () => {
  const unknown = new DaemonRequestError("new daemon error", "future_code", params);
  const incomplete = new DaemonRequestError("incomplete daemon error", "shell_environment_timeout", { shell: "/bin/zsh" });
  expect(errorPresentation(unknown)).toBe(unknown.message);
  expect(errorPresentation(incomplete)).toBe(incomplete.message);
  const container = mount(<><DaemonErrorMessage error={unknown} /><DaemonErrorMessage error={incomplete} /></>);
  expect(container.textContent).toBe("new daemon errorincomplete daemon error");
  expect(container.querySelector("code")).toBeNull();
});

it("sets the path of a conversation that was not found as code, not as quoted text, and does not repeat the ID", () => {
  const error = new DaemonRequestError("plain", "conversation_to_resume_not_found", { agent: "Claude Code", path: "/Users/dev/.claude", id: "3f2b8c1e" });
  expect(errorPresentation(error)).toBe(error);
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(Array.from(container.querySelectorAll("code"), (code) => code.textContent)).toEqual(["/Users/dev/.claude"]);
  // One element, so an alert's flex column cannot split the sentence into an item per literal.
  expect(container.children).toHaveLength(1);
  expect(container.textContent).toBe(`No ${isolate("Claude Code")} conversation with this ID was found in /Users/dev/.claude. Check the ID, and that it was started under this account.`);
});

it("sets resolved project and session names in chips without changing their typeface", () => {
  const daemon = { store: createStateStore({ sessions: new Map([["s1", { id: "s1", title: "Live title" } as import("../protocol").Session]]) }) } as Daemon;
  const error = new DaemonRequestError("plain", "conversation_in_session", { session: "s1" });
  expect(errorPresentation(error)).toBe(error);
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={error} /></DaemonProvider>);
  expect(container.querySelector("[data-inline-chip]")?.textContent).toBe("Live title");
  expect(container.querySelector("code")).toBeNull();
  expect(container.textContent).not.toMatch(/[“”]/);
  act(() => daemon.store.setState({ sessions: new Map([["s1", { id: "s1", title: "Renamed" } as import("../protocol").Session]]) }));
  expect(container.querySelector("[data-inline-chip]")?.textContent).toBe("Renamed");
});

it("keeps a failed launch's name and its nested failure's path as distinct chips", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const params = { title: "Draft", reason_code: "path_not_found", path: "/missing/project" };
  const container = mount(<DaemonProvider value={daemon}><DaemonErrorMessage error={new DaemonRequestError("fallback", "launch_failed", params)} /></DaemonProvider>);
  expect(Array.from(container.querySelectorAll("[data-inline-chip]"), (chip) => chip.textContent)).toEqual(["Draft", "/missing/project"]);
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
  expect(Array.from(container.querySelectorAll("[data-inline-chip]"), (chip) => chip.textContent)).toEqual([...titles, "gone"]);
  expect(container.querySelector("code")).toBeNull();
  expect(container.textContent).not.toContain("old fallback");
  act(() => daemon.store.setState({ sessions: new Map([["s0", { id: "s0", title: "Renamed" } as import("../protocol").Session]]) }));
  expect(Array.from(container.querySelectorAll("[data-inline-chip]"), (chip) => chip.textContent)).toEqual(["Renamed", "s1", "gone"]);
});

it("does not try to parse old or malformed running-session lists out of quoted title text", () => {
  const params = { count: "2", sessions: "“One, task”, “Other”" };
  for (const extra of [{}, { session_ids: "not JSON" }, { session_ids: '["s0",5]' }, { session_ids: "[]" }] as import("../protocol").MessageParams[]) {
    const error = new DaemonRequestError("old daemon fallback", "session_has_running_sessions", { ...params, ...extra });
    expect(errorPresentation(error)).toBe("old daemon fallback");
    const container = mount(<DaemonErrorMessage error={error} />);
    expect(container.textContent).toBe("old daemon fallback");
    expect(container.querySelector("[data-inline-chip]")).toBeNull();
  }
});
