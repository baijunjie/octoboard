// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { TrustedFoldersSection } from "./TrustedFoldersSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);

function mount(element: React.ReactElement) {
  return mountHost(element, { daemon: testDaemon({ state: { trustedDirectories: ["/home/dev/work"], homeDir: "/home/dev" }, request }) });
}

const click = (element: Element | null | undefined) => act(async () => void (element as HTMLElement | undefined)?.click());
const remove = (container: HTMLElement) => container.querySelector<HTMLButtonElement>('button[aria-label^="Stop trusting"]')!;

it("lists a folder without a line of its own under it, beneath only the section lead", () => {
  const container = mount(<TrustedFoldersSection />);
  expect(container.textContent).toContain("~/work");
  expect(container.querySelectorAll("p")).toHaveLength(1);
});

it("asks before removing, saying what is kept, and removes only on confirming", async () => {
  request.mockClear();
  const container = mount(<TrustedFoldersSection />);
  await click(remove(container));
  expect(request).not.toHaveBeenCalled();
  const dialog = document.querySelector('[role="alertdialog"], [role="dialog"]')!;
  expect(dialog.textContent).toContain("Remove trusted folder");
  // Written with the home directory as `~`, as the row it came from is.
  expect(dialog.textContent).toContain("~/work");
  expect(dialog.textContent).not.toContain("/home/dev");
  expect(dialog.textContent).toContain("running sessions and the trust each agent has already recorded are kept");
  await click([...dialog.querySelectorAll("button")].find((button) => button.textContent === "Remove"));
  expect(request).toHaveBeenCalledWith({ type: "remove_trusted_directory", path: "/home/dev/work" });
});

it("removes nothing when the confirmation is cancelled", async () => {
  request.mockClear();
  const container = mount(<TrustedFoldersSection />);
  await click(remove(container));
  const dialog = document.querySelector('[role="alertdialog"], [role="dialog"]')!;
  await click([...dialog.querySelectorAll("button")].find((button) => button.textContent === "Cancel"));
  expect(request).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"], [role="dialog"]')).toBeNull();
});

it("gives the Remove button no description, since its tooltip nearly repeats its name", () => {
  const container = mount(<TrustedFoldersSection />);
  expect(remove(container).getAttribute("aria-describedby")).toBe("");
});

it("leads with what trusting a folder does", () => {
  const container = mount(<TrustedFoldersSection />);
  expect(container.querySelector("p")!.textContent).toContain("trust confirmation");
});
