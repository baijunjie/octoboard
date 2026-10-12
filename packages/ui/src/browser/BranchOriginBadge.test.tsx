// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import type { BranchInfo } from "../protocol";
import { BranchSelect } from "./BranchComparison";
import { mountRows } from "./browserTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const branches: BranchInfo[] = [
  { name: "made-by-session", commit: "a".repeat(40), origin: { source: "hook_diff", session: "s", title: "Fix the layout", created_at: 0 } },
  { name: "made-in-terminal", commit: "b".repeat(40) },
];

const optionOf = (name: string) => [...document.querySelectorAll<HTMLElement>("[role=option]")].find((option) => option.textContent?.startsWith(name))!;

it("marks only a branch with an origin, naming the session that created it", () => {
  const container = mountRows(<BranchSelect label="From" branches={branches} value={undefined} gone={false} onChange={() => {}} />);
  act(() => container.querySelector<HTMLElement>("button")!.click());

  const badge = optionOf("made-by-session").querySelector<HTMLElement>("[role=img]")!;
  expect(badge.getAttribute("aria-label")).toBe("Created by session Fix the layout");
  expect(badge.tabIndex).toBe(-1);
  expect(optionOf("made-in-terminal").querySelector("[role=img]")).toBeNull();
});

it("draws no badge on the trigger", () => {
  const container = mountRows(<BranchSelect label="From" branches={branches} value="made-by-session" gone={false} onChange={() => {}} />);
  expect(container.querySelector("[role=img]")).toBeNull();
});
