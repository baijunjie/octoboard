// @vitest-environment jsdom
import { Alert } from "@heroui/react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { DaemonRequestError } from "../daemon-client";
import { Failure } from "../viewer/viewerStates";
import { ErrorAlert } from "./ErrorAlert";
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

it("uses stock danger parts and only adds the status fill and outer layout classes", () => {
  const { container } = mount(<>
    <Alert status="danger"><Alert.Indicator /><Alert.Content><Alert.Title>Unavailable</Alert.Title><Alert.Description>Quota used up.</Alert.Description></Alert.Content></Alert>
    <ErrorAlert title="Unavailable">Quota used up.</ErrorAlert>
  </>);
  const [stock, outer] = Array.from(container.children);
  const wrapped = outer.querySelector('[data-slot="alert-root"]')!;
  for (const slot of ["indicator", "content", "title", "description"]) {
    const selector = `[data-slot="alert-${slot}"]`;
    expect(wrapped.querySelector(selector)?.tagName).toBe(stock.querySelector(selector)?.tagName);
    expect(wrapped.querySelector(selector)?.className).toBe(stock.querySelector(selector)?.className);
  }
  expect([...wrapped.classList].filter((name) => !stock.classList.contains(name))).toEqual(["alert-fill-danger"]);
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

it("keeps the late announcement on the stock content, with title and reason together", () => {
  const { container, render } = mount(<ErrorAlert title="Work unavailable" announce>Quota used up.</ErrorAlert>);
  const announcement = container.querySelector('[role="alert"]');
  expect(announcement?.getAttribute("data-slot")).toBe("alert-content");
  expect(announcement?.querySelector('[data-slot="alert-title"]')?.textContent).toBe("Work unavailable");
  expect(announcement?.querySelector('[data-slot="alert-description"]')?.textContent).toBe("Quota used up.");
  render(<ErrorAlert title="Work unavailable">Quota used up.</ErrorAlert>);
  expect(container.querySelector('[role="alert"]')).toBeNull();
});

it("retains structured failure diagnostics and recovery outside the Alert", () => {
  const error = new DaemonRequestError("Environment unavailable", "shell_environment_timeout", { shell: "/bin/zsh", command: "env -0", timeout: "10s" });
  const { container } = mount(<Failure title="Could not read the file" message={error} path="README.md" />);
  const alert = container.querySelector('[data-slot="alert-root"]');
  expect(alert?.querySelector('[data-slot="alert-title"]')?.textContent).toBe("Could not read the file");
  expect(alert?.querySelector("pre,ul,h1,h2,h3")).toBeNull();
  expect(alert?.querySelector('[data-slot="alert-description"]')?.querySelector("p,div,pre,ul")).toBeNull();
  expect(alert?.querySelector('[data-slot="alert-description"]')?.textContent).toBeTruthy();
  expect(container.querySelector("code")?.textContent).toBe("/bin/zsh -l -i -c 'env -0'");
  expect(container.textContent).toContain("10s");
  expect(errorAlerts.getSnapshot()[0].error).toBe(error);
});
