// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { InlineChip } from "../components/InlineChip";
import { format, globalParam } from "./catalog";
import { currentLanguage, setLanguage } from "./language";
import { Message } from "./react";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it.each(["en", "zh-Hans", "ar"] as const)("keeps fixed product references rich and their plain companion usable (%s)", (language) => {
  const before = currentLanguage();
  setLanguage(language);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); setLanguage(before); });
  for (const [id, expected] of [
    ["settings.accounts.dirHintGrok", ["Grok", "Grok"]],
    ["settings.accounts.approval.codexDescription", ["Codex"]],
    ["settings.accounts.approval.claudeDescription", ["Claude Code"]],
    ["settings.permissions.lead", ["Octoboard", "Octoboard", "macOS"]],
    ["settings.cloneDir.description.gitUrl", ["Git"]],
    ["git.unavailable", ["Git"]],
    ["git.notARepository", ["Git"]],
    ["settings.permissions.screenRecording.refused", ["macOS", language === "zh-Hans" ? "系统设置" : "System Settings"]],
    ["settings.permissions.accessibility.refused", ["Octoboard", language === "zh-Hans" ? "系统设置" : "System Settings"]],
  ] as const) {
    act(() => root.render(<Message id={id} />));
    expect(container.textContent).toBe(format(language, id));
    expect([...container.querySelectorAll("[data-inline-chip]")].map((chip) => chip.textContent).sort()).toEqual([...expected].sort());
    for (const chip of container.querySelectorAll("[data-inline-chip]")) {
      expect(chip.getAttribute("dir")).toBe("auto");
      expect(chip.getAttribute("data-size")).toBe("xs");
      expect(chip.querySelector("code")).toBeNull();
      expect(chip.textContent).not.toMatch(/[\u2066-\u2069]/);
    }
    expect(format(language, id)).not.toMatch(/\{\w+\}|<[^>]+>/);
  }
  for (const id of ["branchCleanup.consequence.force", "dialog.sessionBranches.force"] as const) {
    for (const count of [1, 3]) {
      act(() => root.render(<Message id={id} params={{ count }} />));
      expect([...container.querySelectorAll("[data-inline-chip]")].map((chip) => chip.textContent)).toEqual(["Git"]);
      expect(container.textContent).toBe(format(language, id, { count }));
    }
  }
  act(() => root.render(<Message id="startup.noAddress.shell" params={{ port: <InlineChip monospace wrap>?port=</InlineChip> }} />));
  expect([...container.querySelectorAll("[data-inline-chip]")].map((chip) => chip.textContent)).toEqual(["Tauri", "?port="]);
  const name = "مراجعة/中文/Work";
  act(() => root.render(<Message id="dialog.session.ownerFixed" params={{ name: <InlineChip dir="auto" wrap>{name}</InlineChip> }} />));
  expect(container.querySelector("[data-inline-chip]")?.textContent).toBe(name);
  expect(container.textContent).toBe(format(language, "dialog.session.ownerFixed", { name }));
  // Whole labels can deliberately supply their existing presentation instead of a referenced product chip.
  act(() => root.render(<Message id="history.speaker.octoboard" params={{ appName: globalParam("appName") } as never} />));
  expect(container.querySelector("[data-inline-chip]")).toBeNull();
});
