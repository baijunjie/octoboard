// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { AGENT_OPTIONS } from "./agents";
import { SessionAgentTooltip } from "./components/SessionAgentIcon";
import { accountOf, consoleOf, gitStatusOf, projectOf, sessionOf } from "./gallery/fixtures/builders";
import { gitBadgeContent } from "./gitStatusCopy";
import { gitBadgeAriaLabel } from "./gitStatusLabel";
import { currentLanguage, setLanguage, t } from "./i18n/language";
import { sessionLocationContent } from "./sessionCopy";
import { sessionLocation } from "./sessionLabel";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(content: React.ReactNode): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(content));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return container;
}

const bare = (text: string) => text.replace(/[\u2068\u2069]/g, "");

it.each(["en", "zh-Hans", "ar"] as const)("keeps rich location wording equal to plain copy without embedding string isolates (%s)", (language) => {
  const before = currentLanguage();
  setLanguage(language);
  onTestFinished(() => setLanguage(before));
  const owner = consoleOf("c", "مركز Work 中文");
  const project = projectOf("p", "c", "مشروع Project 中文");
  const consoles = new Map([[owner.id, owner]]);
  const projects = new Map([[project.id, project]]);
  for (const projectId of [project.id, "gone", undefined]) {
    const session = sessionOf("s", owner.id, projectId, "Task", "idle");
    const container = mount(sessionLocationContent(t, session, consoles, projects));
    expect(container.textContent).toBe(bare(sessionLocation(t, session, consoles, projects)));
    // A project's name is the whole title, so it is no chip; a console's name runs inside a sentence, so it is one.
    expect(container.querySelector("[data-inline-reference]") === null).toBe(projectId === project.id || projectId === "gone");
    for (const chip of container.querySelectorAll("[data-inline-reference]")) {
      expect(chip.getAttribute("dir")).toBe("auto");
      expect(chip.querySelector("code")).toBeNull();
    }
  }
});

it.each(AGENT_OPTIONS)("retains account qualifiers and removed-account paths for $label", ({ value: agent, label }) => {
  for (const account of [undefined, accountOf("stored", "claude", "العمل Work 中文", "/home/stored")]) {
    const session = sessionOf("s", "c", "p", "Task", "idle", { agent, account_id: account?.id ?? "gone", config_dir: "/Users/中文/old account" });
    const container = mount(<SessionAgentTooltip session={session} accounts={account ? [{ ...account, agent }] : []} defaultNames={{}} />);
    expect(container.querySelector("[data-inline-reference]")).toBeNull();
    expect(container.firstElementChild!.firstElementChild!.textContent).toBe(label);
    expect(container.textContent).toContain(account?.name ?? session.config_dir);
    expect(container.querySelector("code")?.textContent).toBe(account ? undefined : session.config_dir);
  }
});

it.each(["en", "zh-Hans", "ar"] as const)("keeps every Git phase the same in rich and plain routes, Git's own text in neither (%s)", (language) => {
  const before = currentLanguage();
  setLanguage(language);
  onTestFinished(() => setLanguage(before));
  for (const activity of ["idle", "checking", "pulling", "pushing"] as const) {
    for (const branch of ["feature/中文/مرحبا", null]) {
      const status = gitStatusOf("p", { activity, branch, ahead: 2, behind: 1, error: "fatal: failed\n    /Users/中文/مرحبا" });
      const container = mount(gitBadgeContent(language, t, status));
      expect(container.textContent).toBe(bare(gitBadgeAriaLabel(language, t, status)));
      // A failed check's own text is the failure mark's popover's alone.
      expect(container.querySelector("pre")).toBeNull();
      expect(container.textContent).not.toContain("fatal");
      expect(gitBadgeAriaLabel(language, t, status)).not.toContain("fatal");
      expect(Array.from(container.querySelectorAll("[data-inline-reference]")).filter((chip) => chip.querySelector("code"))).toHaveLength(branch ? 1 : 0);
    }
  }
});
