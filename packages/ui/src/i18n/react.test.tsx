// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { InlineReference } from "../components/InlineReference";
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
    ["settings.accounts.approval.codexReviewDescription", ["Codex"]],
    ["settings.accounts.approval.claudeReviewDescription", ["Claude Code"]],
    ["settings.permissions.lead", ["Octoboard", "Octoboard", "macOS"]],
    ["settings.cloneDir.description.gitUrl", ["Git"]],
    ["git.unavailable", ["Git"]],
    ["git.notARepository", ["Git"]],
    ["settings.permissions.screenRecording.refused", ["macOS", language === "zh-Hans" ? "系统设置" : "System Settings"]],
    ["settings.permissions.accessibility.refused", ["Octoboard", language === "zh-Hans" ? "系统设置" : "System Settings"]],
  ] as const) {
    act(() => root.render(<Message id={id} />));
    expect(container.textContent).toBe(format(language, id));
    expect([...container.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent).sort()).toEqual([...expected].sort());
    for (const chip of container.querySelectorAll("[data-inline-reference]")) {
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
      expect([...container.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toEqual(["Git"]);
      expect(container.textContent).toBe(format(language, id, { count }));
    }
  }
  act(() => root.render(<Message id="startup.noAddress.shell" params={{ port: <InlineReference monospace wrap>?port=</InlineReference> }} />));
  expect([...container.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toEqual(["Tauri", "?port="]);
  const name = "مراجعة/中文/Work";
  act(() => root.render(<Message id="dialog.session.ownerFixed" params={{ name: <InlineReference dir="auto" wrap>{name}</InlineReference> }} />));
  expect(container.querySelector("[data-inline-reference]")?.textContent).toBe(name);
  expect(container.textContent).toBe(format(language, "dialog.session.ownerFixed", { name }));
  // A caller can supply its own product-name presentation instead of a referenced product chip.
  act(() => root.render(<Message id="app.quit.title" params={{ appName: globalParam("appName") } as never} />));
  expect(container.querySelector("[data-inline-reference]")).toBeNull();
});

/** Every text node under `root`. */
function textNodes(root: Node): Text[] {
  const found: Text[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) found.push(node as Text);
  return found;
}

it.each(["en", "zh-Hans"] as const)("joins two sentences without a whitespace-only text node (%s)", (language) => {
  const before = currentLanguage();
  setLanguage(language);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); setLanguage(before); });
  const cause = <Message id="daemon.sharing.reason.busy" />;
  const fix = <Message id="daemon.sharing.fix.other" />;
  act(() => root.render(<Message id="daemon.sharing.withFix" params={{ cause, fix }} />));
  expect(container.textContent).toBe(format(language, "daemon.sharing.withFix", { cause: format(language, "daemon.sharing.reason.busy"), fix: format(language, "daemon.sharing.fix.other") }));
  expect(textNodes(container).filter((node) => node.data.trim() === "")).toEqual([]);
  // A separator before an element that is not a message cannot be merged, but the text stays the same.
  act(() => root.render(<Message id="daemon.sharing.withFix" params={{ cause, fix: <b>fixed</b> }} />));
  expect(container.textContent).toBe(`${format(language, "daemon.sharing.reason.busy")}${language === "en" ? " " : ""}fixed`);
});
