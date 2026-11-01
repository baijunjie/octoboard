// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { gitStatusOf } from "../fixtures/builders";
import { currentLanguage, setLanguage, t } from "../i18n/language";
import type { GitStatus } from "../protocol";
import { gitBadgeContent } from "./gitStatusCopy";
import { gitBadgeAriaLabel } from "./gitStatusLabel";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const bare = (text: string) => text.replace(/[⁨⁩]/g, "");

const COUNTS = { ahead: 2, behind: 1 };
const STATES: [string, Partial<GitStatus>][] = [
  ["idle on a branch", { branch: "feature/x" }],
  ["idle, branchless", { branch: null }],
  ["idle with an empty branch name", { branch: "" }],
  ["detached", { branch: "a1b2c3d", detached: true }],
  ["checking on a branch", { activity: "checking", branch: "feature/x" }],
  ["checking, branch unknown", { activity: "checking", branch: null }],
  ["pulling on a branch", { activity: "pulling", branch: "feature/x" }],
  ["pulling, branch unknown", { activity: "pulling", branch: null }],
  ["pushing on a branch", { activity: "pushing", branch: "feature/x" }],
  ["pushing, branch unknown", { activity: "pushing", branch: null }],
  ["failed, without a branch", { branch: null, error: "fatal: failed" }],
  ["failed, on a branch", { branch: "feature/x", error: "fatal: failed" }],
  ["ahead", { branch: "feature/x", ahead: COUNTS.ahead }],
  ["behind", { branch: "feature/x", behind: COUNTS.behind }],
  ["ahead and behind", { branch: "feature/x", ...COUNTS }],
  ["ahead and behind, branchless", { branch: null, ...COUNTS }],
];

function mount(node: React.ReactNode): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(node));
  return container;
}

const LANGUAGES = ["en", "zh-Hans", "ar"] as const;

it.each(LANGUAGES.flatMap((language) => STATES.map(([name, extra]) => ({ language, name, extra }))))(
  "words the tooltip as the accessible name does: $name ($language)",
  ({ language, extra }) => {
    const before = currentLanguage();
    setLanguage(language);
    onTestFinished(() => setLanguage(before));
    const status = gitStatusOf("p", extra);
    const container = mount(gitBadgeContent(language, t, status));
    expect(container.textContent).toBe(bare(gitBadgeAriaLabel(language, t, status)));
  },
);
