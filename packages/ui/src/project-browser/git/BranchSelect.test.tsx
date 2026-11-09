// @vitest-environment jsdom
import { act, useState } from "react";
import { expect, it, vi } from "vitest";

import type { BranchInfo } from "../../protocol";
import { BranchSelect } from "./BranchSelect";
import { mountRows } from "../browserTestSupport";

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


it("shows loading in place of an empty branch choice and returns to its normal prompt when ready", () => {
  let setLoading!: (loading: boolean) => void;
  function Field() {
    const [loading, update] = useState(true);
    setLoading = update;
    return <BranchSelect label="From" branches={[]} value={undefined} gone={false} isLoading={loading} selectedSessionId={undefined} onChange={() => {}} />;
  }
  const container = mountRows(<Field />);
  const trigger = container.querySelector<HTMLButtonElement>("[data-slot=select-trigger]")!;
  expect(trigger.textContent).toBe("Loading…");
  expect(trigger.disabled).toBe(true);
  expect(trigger.getAttribute("aria-busy")).toBe("true");
  expect(trigger.querySelector("[data-slot=spinner]")).not.toBeNull();
  expect(trigger.querySelector("[data-slot=select-default-indicator]")).toBeNull();
  act(() => setLoading(false));
  expect(trigger.textContent).toBe("Choose a branch");
  expect(trigger.disabled).toBe(false);
  expect(trigger.querySelector("[data-slot=spinner]")).toBeNull();
  expect(trigger.querySelector("[data-slot=select-default-indicator]")).not.toBeNull();
});

it("retains a branch name while its choices load and does not allow changing it", () => {
  const changed = vi.fn();
  const container = mountRows(<BranchSelect label="From" branches={branches} value={RTL_BRANCH} gone={false} isLoading selectedSessionId={undefined} onChange={changed} />);
  const trigger = container.querySelector<HTMLButtonElement>("[data-slot=select-trigger]")!;
  expect(trigger.textContent).toBe(RTL_BRANCH);
  expect(trigger.disabled).toBe(true);
  act(() => trigger.click());
  expect(document.querySelector("[role=listbox]")).toBeNull();
  expect(changed).not.toHaveBeenCalled();
});
