// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { openDetails } from "./detailsTestSupport";
import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { mountForTooltip } from "./tooltipTestSupport";
import { PaneFailure } from "./PaneFailure";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

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

it("holds the rest of the failure behind Details, beside Try again, and has no Try again without a retry", () => {
  const error = new DaemonRequestError("Environment unavailable", "shell_environment_timeout", { shell: "/bin/zsh", command: "env -0", timeout: "10s" });
  const container = mountForTooltip(<PaneFailure title="Could not read the file" message={error} announce={false} onRetry={() => {}} />);
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(panel.querySelector("code")).toBeNull();
  expect([...panel.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Try again", "Details"]);
  expect(openDetails(panel).querySelector("code")?.textContent).toBe("/bin/zsh -l -i -c 'env -0'");
  const alone = mountForTooltip(<PaneFailure title="History unavailable" summary="Could not be read." details={<p>Reopen it.</p>} announce={false} />);
  expect([...alone.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Details"]);
});

it("announces the title and the line only, and not the buttons", () => {
  const container = mountForTooltip(<PaneFailure title="Could not list the files" message="Permission denied" announce onRetry={() => {}} />);
  expect(container.querySelector("[role=alert] button")).toBeNull();
});

it.each(["internal_error", "unexpected_error", "some_code_from_a_newer_daemon"])("shows the generic sentence for %s, and the raw message only behind Details", (code) => {
  const error = new DaemonRequestError("database is locked", code, { detail: "database is locked" });
  const container = mountForTooltip(<PaneFailure title="Could not load the pages" message={error} announce onRetry={() => {}} />);
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(panel.querySelector("p")?.textContent).toBe("Something unexpected went wrong.");
  expect(panel.textContent).not.toContain("database is locked");
  expect(container.querySelector("[role=alert]")?.textContent).toBe("Could not load the pagesSomething unexpected went wrong.");
  expect(openDetails(panel).textContent).toBe("database is locked");
});

it("keeps the application's own remedy under the line, outside the announcement, with no Details when there is no raw text", () => {
  const container = mountForTooltip(<PaneFailure title="History unavailable" summary="Could not be read." remedy="Reopen it." announce />);
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect([...panel.querySelectorAll("p")].map((p) => p.textContent)).toEqual(["Could not be read.", "Reopen it."]);
  expect(container.querySelector("[role=alert]")?.textContent).toBe("History unavailableCould not be read.");
  expect(panel.querySelector("button")).toBeNull();
});
