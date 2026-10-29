// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { mountForTooltip } from "./tooltipTestSupport";
import { PaneFailure } from "./PaneFailure";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The surface under the panel's lines holding the rest of the failure, if it has one. */
function details(panel: Element): Element | null {
  return panel.querySelector(':scope > [data-slot="surface"]');
}

it("holds the title, the reason and Try again in the empty-state shape, hands the retry the pane's element, and opens no error alert dialog", () => {
  const onRetry = vi.fn();
  const container = mountForTooltip(<PaneFailure title="Could not list the files" message="Permission denied" announce={false} onRetry={onRetry} />);
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(panel.querySelector("h2")?.textContent).toBe("Could not list the files");
  expect(panel.querySelector("p")?.textContent).toBe("Permission denied");
  expect(panel.querySelector("svg")?.parentElement?.className).toContain("text-danger-soft-foreground");
  expect(container.querySelector("[role=alert]")).toBeNull();
  expect([...panel.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Try again"]);
  act(() => panel.querySelector("button")!.click());
  expect(onRetry).toHaveBeenCalledWith(container.firstElementChild);
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("announces itself when told the pane has painted", () => {
  const container = mountForTooltip(<PaneFailure title="Could not list the files" message="Permission denied" announce onRetry={() => {}} />);
  expect(container.querySelector("[role=alert]")?.textContent).toBe("Could not list the filesPermission denied");
});

it("holds the rest of the failure on a surface under the lines, with Try again the only button, and has no Try again without a retry", () => {
  const error = new DaemonRequestError("Environment unavailable", "shell_environment_timeout", { shell: "/bin/zsh", command: "env -0", timeout: "10s" });
  const container = mountForTooltip(<PaneFailure title="Could not read the file" message={error} announce onRetry={() => {}} />);
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect([...panel.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Try again"]);
  expect(details(panel)?.querySelector("code")?.textContent).toBe("/bin/zsh -l -i -c 'env -0'");
  expect(container.querySelector("[role=alert]")?.querySelector("code")).toBeNull();
  const alone = mountForTooltip(<PaneFailure title="History unavailable" summary="Could not be read." details={<p>Reopen it.</p>} announce={false} />);
  expect(alone.querySelector("button")).toBeNull();
  expect(details(alone.querySelector('[data-slot="empty-state"]')!)?.textContent).toBe("Reopen it.");
});

it.each(["internal_error", "unexpected_error", "some_code_from_a_newer_daemon"])("shows the generic sentence for %s, and the raw message only on the details surface", (code) => {
  const error = new DaemonRequestError("database is locked", code, { detail: "database is locked" });
  const container = mountForTooltip(<PaneFailure title="Could not load the pages" message={error} announce onRetry={() => {}} />);
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(panel.querySelector("p")?.textContent).toBe("Something unexpected went wrong.");
  expect(container.querySelector("[role=alert]")?.textContent).toBe("Could not load the pagesSomething unexpected went wrong.");
  expect(details(panel)?.textContent).toBe("database is locked");
  // Raw text wraps in the narrow surface rather than scrolling sideways.
  expect(details(panel)?.querySelector("pre")?.className).toContain("whitespace-pre-wrap");
});

it("keeps the application's own remedy under the line, outside the announcement, with no details surface when there is no raw text", () => {
  const container = mountForTooltip(<PaneFailure title="History unavailable" summary="Could not be read." remedy="Reopen it." announce />);
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect([...panel.querySelectorAll("p")].map((p) => p.textContent)).toEqual(["Could not be read.", "Reopen it."]);
  expect(container.querySelector("[role=alert]")?.textContent).toBe("History unavailableCould not be read.");
  expect(panel.querySelector("button")).toBeNull();
  expect(details(panel)).toBeNull();
});

/** Mounts a failure with every element measuring `scrollHeight` tall in a `clientHeight` box, since jsdom has no
 * layout and the scroller's region follows those, and returns the scroller. */
async function scroller(sizes: { scrollHeight: number; clientHeight: number }): Promise<HTMLElement> {
  const restore = (["scrollHeight", "clientHeight"] as const).map((key) => {
    const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, key);
    Object.defineProperty(HTMLElement.prototype, key, { configurable: true, get: () => sizes[key] });
    return () => (original ? Object.defineProperty(HTMLElement.prototype, key, original) : delete (HTMLElement.prototype as unknown as Record<string, unknown>)[key]);
  });
  onTestFinished(() => restore.forEach((undo) => undo()));
  const container = mountForTooltip(<PaneFailure title="Could not list the files" message="Permission denied" announce={false} onRetry={() => {}} />);
  // HeroUI writes the fade's state on the next frame.
  await act(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
  return container.querySelector<HTMLElement>("[data-slot=scroll-shadow]")!;
}

it("scrolls in its place as a region named by its heading while it is taller than the pane", async () => {
  const scrolling = await scroller({ scrollHeight: 1000, clientHeight: 100 });
  expect(scrolling.getAttribute("role")).toBe("region");
  expect(document.getElementById(scrolling.getAttribute("aria-labelledby")!)?.textContent).toBe("Could not list the files");
  expect(scrolling.tabIndex).toBe(0);
});

it("is no region while it fits", async () => {
  const fitting = await scroller({ scrollHeight: 100, clientHeight: 100 });
  expect(fitting.hasAttribute("role")).toBe(false);
  expect(fitting.hasAttribute("tabindex")).toBe(false);
});
