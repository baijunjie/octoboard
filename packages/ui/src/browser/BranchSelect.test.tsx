// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import type { BranchInfo } from "../protocol";
import { BranchSelect } from "./BranchSelect";
import { mountRows } from "./browserTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Starts with right-to-left characters, so a label left to resolve its own direction would read
// right to left and dock its kept end on the other side.
const RTL_BRANCH = "مراجعة/feature";
const branches: BranchInfo[] = [
  { name: RTL_BRANCH, commit: "a".repeat(40) },
  { name: "main", commit: "b".repeat(40) },
];

it("lays every branch name out left to right, in the list as on the trigger", () => {
  const container = mountRows(
    <BranchSelect label="From" branches={branches} value={RTL_BRANCH} gone={false} selectedSessionId={undefined} onChange={() => {}} />,
  );
  const trigger = container.querySelector<HTMLElement>("button")!;
  const triggerLabel = [...trigger.querySelectorAll<HTMLElement>("[dir]")].find((node) => node.textContent === RTL_BRANCH);
  expect(triggerLabel?.getAttribute("dir")).toBe("ltr");

  act(() => trigger.click());
  const options = [...document.querySelectorAll<HTMLElement>("[role=option]")];
  expect(options).toHaveLength(branches.length);
  for (const option of options) {
    const name = option.querySelector<HTMLElement>("[dir]")!;
    expect(branches.map((branch) => branch.name)).toContain(name.textContent);
    expect(name.getAttribute("dir")).toBe("ltr");
  }
});
