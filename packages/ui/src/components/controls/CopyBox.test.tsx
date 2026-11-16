// @vitest-environment jsdom
import { expect, it, vi } from "vitest";
import { act } from "react";

import type { PlatformAdapter } from "../../platform";
import { BROWSER_HOST } from "../../test/hosts";
import { mountHost } from "../../test/mountHost";
import { CopyCommand } from "./CopyBox";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** What the tests vary of a `CopyCommand`: its text, what it shows instead, and whether it scrolls instead of wrapping. */
type Props = { text?: string; content?: React.ReactNode } & ({ wrap?: undefined } | { wrap: false; scrollLabel: string });

function mount(props: Props = {}, writeText: PlatformAdapter["clipboard"] = { writeText: vi.fn(async () => {}) }) {
  return mountHost(
    <CopyCommand text="ssh me@host" copyText="Copy command" failedText="Could not copy" {...props} />,
    { platform: { kind: "browser", host: BROWSER_HOST, clipboard: writeText } },
  );
}

const copy = (container: HTMLElement) => container.querySelector<HTMLButtonElement>('button[aria-label="Copy command"]')!;

it("shows its text and copies it", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const container = mount({}, { writeText });
  expect(container.querySelector("pre")?.textContent).toBe("ssh me@host");
  await act(async () => copy(container).click());
  expect(writeText).toHaveBeenCalledWith("ssh me@host");
});

it("shows its content in place of the text where it has some, and still copies the text", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const container = mount({ text: "ssh me@10.0.0.5", content: <>ssh me@<button type="button" data-token="">10.0.0.5</button></> }, { writeText });
  const shown = container.querySelector("pre")!;
  expect(shown.textContent).toBe("ssh me@10.0.0.5");
  expect(shown.querySelector("[data-token]")).not.toBeNull();
  await act(async () => copy(container).click());
  expect(writeText).toHaveBeenCalledWith("ssh me@10.0.0.5");
});

it("copies the text, not the content, when the two differ", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const container = mount({ text: "the copied text", content: <span data-shown="">what is shown</span> }, { writeText });
  expect(container.querySelector("pre")?.textContent).toBe("what is shown");
  expect(container.textContent).not.toContain("the copied text");
  await act(async () => copy(container).click());
  expect(writeText).toHaveBeenCalledWith("the copied text");
});

it("shows its content in the sideways-scrolling form too", () => {
  const container = mount({ content: <span data-shown="">shown</span>, wrap: false, scrollLabel: "Command" });
  expect(container.querySelector("[data-shown]")?.closest("pre")).not.toBeNull();
  expect(container.querySelector("pre")?.textContent).toBe("shown");
});
