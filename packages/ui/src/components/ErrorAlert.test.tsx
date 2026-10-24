// @vitest-environment jsdom
import { Alert } from "@heroui/react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { DaemonRequestError } from "../daemon-client";
import { ViewerErrorMessage } from "../viewer/ViewerErrorMessage";
import { detailsButton, openDetails } from "./detailsTestSupport";
import { AlertButton } from "./AlertButton";
import { ErrorAlert } from "./ErrorAlert";
import { FailureAlert } from "./FailureAlert";
import { InlineReference } from "./InlineReference";
import { TintedAlert } from "./TintedAlert";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(children: React.ReactNode) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  });
  act(() => root.render(children));
  return { container, render: (children: React.ReactNode) => act(() => root.render(children)) };
}

it("uses stock danger parts and only adds the status fill and the classes that let its buttons wrap beneath the text", () => {
  const { container } = mount(<>
    <Alert status="danger"><Alert.Indicator /><Alert.Content><Alert.Title>Unavailable</Alert.Title><Alert.Description>Quota used up.</Alert.Description></Alert.Content></Alert>
    <ErrorAlert title="Unavailable">Quota used up.</ErrorAlert>
  </>);
  const [stock, outer] = Array.from(container.children);
  const wrapped = outer.querySelector('[data-slot="alert-root"]')!;
  for (const slot of ["indicator", "content", "title", "description"]) {
    const selector = `[data-slot="alert-${slot}"]`;
    expect(wrapped.querySelector(selector)?.tagName).toBe(stock.querySelector(selector)?.tagName);
    if (slot === "content") continue;
    expect(wrapped.querySelector(selector)?.className).toBe(stock.querySelector(selector)?.className);
  }
  const content = (root: Element) => root.querySelector('[data-slot="alert-content"]')!;
  expect([...content(wrapped).classList].filter((name) => !content(stock).classList.contains(name))).toEqual(["min-w-40", "basis-0"]);
  expect([...wrapped.classList].filter((name) => !stock.classList.contains(name))).toEqual(["alert-fill-danger", "flex-wrap"]);
  expect(wrapped.querySelector("h1,h2,h3")).toBeNull();
  expect(wrapped.querySelector('[role="alert"]')).toBeNull();
});

it.each(["default", "accent", "success", "warning", "danger"] as const)("keeps every stock part for %s status", (status) => {
  const parts = <><Alert.Indicator /><Alert.Content><Alert.Title>Account unavailable</Alert.Title><Alert.Description>Quota used up.</Alert.Description></Alert.Content></>;
  const { container } = mount(<><Alert status={status}>{parts}</Alert><TintedAlert status={status}>{parts}</TintedAlert></>);
  const [stock, wrapped] = Array.from(container.children);
  expect(wrapped.classList.contains(`alert-fill-${status}`)).toBe(true);
  expect(wrapped.className.split(" ").filter((name) => name !== `alert-fill-${status}`).join(" ").trim()).toBe(stock.className);
  expect(wrapped.innerHTML).toBe(stock.innerHTML);
  expect(wrapped.getAttribute("style")).toBe(stock.getAttribute("style"));
  for (const slot of ["indicator", "content", "title", "description"]) {
    expect(wrapped.querySelector(`[data-slot="alert-${slot}"]`)).not.toBeNull();
  }
});

it("announces the title and the description together, on a wrapper of its own and not on the stock content", () => {
  const { container, render } = mount(
    <ErrorAlert title="Work unavailable" announce details={<p>Free some disk space.</p>} actions={<button type="button">Try again</button>}>
      Quota used up.
    </ErrorAlert>,
  );
  const announcement = container.querySelector('[role="alert"]')!;
  expect(announcement.getAttribute("data-slot")).toBeNull();
  expect(announcement.parentElement?.getAttribute("data-slot")).toBe("alert-content");
  expect(announcement.querySelector('[data-slot="alert-title"]')?.textContent).toBe("Work unavailable");
  expect(announcement.querySelector('[data-slot="alert-description"]')?.textContent).toBe("Quota used up.");
  // Never the details or the actions.
  expect(announcement.textContent).toBe("Work unavailableQuota used up.");
  expect(container.querySelectorAll('[role="alert"]')).toHaveLength(1);
  // It is read as the Alert mounts: a later change of the prop does not turn it on or off.
  render(<ErrorAlert title="Work unavailable">Quota used up.</ErrorAlert>);
  expect(container.querySelector('[role="alert"]')).not.toBeNull();
});

it("does not announce a failure that was there as its view opened", () => {
  const { container, render } = mount(<ErrorAlert title="Work unavailable">Quota used up.</ErrorAlert>);
  expect(container.querySelector('[role="alert"]')).toBeNull();
  render(<ErrorAlert title="Work unavailable" announce>Quota used up.</ErrorAlert>);
  expect(container.querySelector('[role="alert"]')).toBeNull();
});

it("keeps the structured diagnostics of a failure behind the Alert's Details, and opens no error alert dialog for it", () => {
  const error = new DaemonRequestError("Environment unavailable", "shell_environment_timeout", { shell: "/bin/zsh", command: "env -0", timeout: "10s" });
  const { container } = mount(<FailureAlert title="Could not read the file" announce={false} message={error} summary={<ViewerErrorMessage error={error} path="README.md" />} />);
  const alert = container.querySelector('[data-slot="alert-root"]');
  expect(alert?.querySelector('[data-slot="alert-title"]')?.textContent).toBe("Could not read the file");
  expect(alert?.querySelector('[data-slot="alert-description"]')?.querySelector("p,div,pre,ul")).toBeNull();
  expect(alert?.querySelector('[data-slot="alert-description"]')?.textContent).toBeTruthy();
  // The sentence of what happened is all that is in the Alert until Details is opened.
  expect(alert?.querySelector("code")).toBeNull();
  const dialog = openDetails(alert!);
  expect(dialog.querySelector("code")?.textContent).toBe("/bin/zsh -l -i -c 'env -0'");
  expect(dialog.textContent).toContain("10s");
  expect(container.querySelector('[data-slot="alert-description"] [role="dialog"]')).toBeNull();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("ends with the recovery action and a Details button that opens the rest, outside the announced description", () => {
  const { container } = mount(
    <ErrorAlert title="Could not load" announce details={<p>Free some disk space.</p>} actions={<button type="button">Try again</button>}>
      Disk full.
    </ErrorAlert>,
  );
  const root = container.querySelector('[data-slot="alert-root"]')!;
  const content = root.querySelector('[data-slot="alert-content"]')!;
  expect(content.querySelector('[data-slot="alert-description"]')?.textContent).toBe("Disk full.");
  expect(content.querySelector("button")).toBeNull();
  // The buttons are where HeroUI's own Alert puts its action: after the content, in the Alert's row.
  expect([...root.children].map((child) => child.getAttribute("data-slot") ?? child.tagName)).toEqual(["alert-indicator", "alert-content", "DIV"]);
  expect([...root.lastElementChild!.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Details", "Try again"]);
  expect(root.textContent).not.toContain("Free some disk space.");
  expect(container.querySelector('[role="alert"]')?.textContent).toBe("Could not loadDisk full.");
  const dialog = openDetails(root);
  expect(dialog.textContent).toBe("Free some disk space.");
  expect(dialog.closest(".control-fills")).not.toBeNull();
  expect(container.querySelector('[role="alert"]')?.textContent).toBe("Could not loadDisk full.");
});

it("gives focus back to the Details button when Escape closes the popover", () => {
  const { container } = mount(<ErrorAlert details={<p>Free some disk space.</p>}>Disk full.</ErrorAlert>);
  const button = detailsButton(container)!;
  act(() => button.focus());
  const dialog = openDetails(container);
  act(() => void dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(document.activeElement).toBe(button);
});

it("has no Details button when there is nothing more to say", () => {
  const { container } = mount(
    <>
      <ErrorAlert actions={<button type="button">Try again</button>}>Disk full.</ErrorAlert>
      <FailureAlert announce={false} message="Permission denied" onRetry={() => {}} />
    </>,
  );
  expect(detailsButton(container)).toBeUndefined();
  expect(container.querySelectorAll("button")).toHaveLength(2);
  const coded = new DaemonRequestError("raw", "git_failed", { detail: "fatal: not a git repository" });
  const { container: other } = mount(<FailureAlert announce={false} message={coded} />);
  expect(openDetails(other).textContent).toContain("fatal: not a git repository");
});

it("draws a warning with the warning status and the same parts", () => {
  const { container } = mount(<ErrorAlert status="warning" actions={<button type="button">Retry</button>}>Check skipped.</ErrorAlert>);
  const root = container.querySelector('[data-slot="alert-root"]')!;
  expect(root.classList.contains("alert-fill-warning")).toBe(true);
  expect(root.classList.contains("alert-fill-danger")).toBe(false);
});

it("names the wrapper by id for a field it describes", () => {
  const { container } = mount(<ErrorAlert id="models-alert">No list.</ErrorAlert>);
  expect(container.querySelector("#models-alert")?.querySelector('[data-slot="alert-root"]')).not.toBeNull();
});

it("draws a danger Alert's recovery action solid and its Details soft, in the danger variants", () => {
  // A failure the application has no words for keeps its raw text behind Details.
  const { container } = mount(<FailureAlert announce={false} message={new DaemonRequestError("Permission denied", "unexpected_error", {})} onRetry={() => {}} />);
  const buttons = [...container.querySelectorAll("button")];
  expect(buttons.map((button) => button.textContent)).toEqual(["Details", "Try again"]);
  const [details, retry] = buttons;
  expect(retry.className).toContain("button--danger");
  expect(retry.className).not.toContain("button--danger-soft");
  expect(details.className).toContain("button--danger-soft");
});

it("draws a warning Alert's buttons from the warning tokens, solid for the action and soft for Details", () => {
  const { container } = mount(<ErrorAlert status="warning" actions={<AlertButton kind="action">Try again</AlertButton>} details={<p>More</p>}>Slow</ErrorAlert>);
  // Details first, the recovery action at the trailing edge.
  const [details, retry] = [...container.querySelectorAll("button")];
  expect(retry.className).toContain("alert-button--warning");
  expect(retry.className).not.toContain("alert-button--warning-soft");
  expect(details.textContent).toBe("Details");
  expect(details.className).toContain("alert-button--warning-soft");
});

it("shows the remedy under the description, outside the announcement, and gives an error with no raw text no Details", () => {
  const { container } = mount(<ErrorAlert title="Could not save" announce remedy="Free some disk space.">Nothing was written.</ErrorAlert>);
  expect(container.querySelector("[role=alert]")?.textContent).toBe("Could not saveNothing was written.");
  expect([...container.querySelectorAll('[data-slot="alert-description"]')].map((node) => node.textContent)).toEqual(["Nothing was written.", "Free some disk space."]);
  expect(detailsButton(container)).toBeUndefined();
});

it("puts the raw message of an error with no catalog message behind Details and says the generic sentence", () => {
  const { container } = mount(<FailureAlert announce={false} title="Could not read" message={new DaemonRequestError("database is locked", "internal_error", {})} />);
  const alert = container.querySelector('[data-slot="alert-root"]')!;
  expect(alert.querySelector('[data-slot="alert-description"]')?.textContent).toBe("Something unexpected went wrong.");
  expect(alert.textContent).not.toContain("database is locked");
  expect(openDetails(container).textContent).toBe("database is locked");
});

it("leaves the Alert's tone behind in the Details popover, where a chip takes its own colour", () => {
  const { container } = mount(
    <ErrorAlert status="warning" details={<InlineReference>skills/</InlineReference>}>
      Slow: <InlineReference>plugins/</InlineReference>
    </ErrorAlert>,
  );
  expect(container.querySelector("[data-inline-reference]")?.getAttribute("data-color")).toBe("warning");
  expect(openDetails(container).querySelector("[data-inline-reference]")?.getAttribute("data-color")).toBe("default");
});
