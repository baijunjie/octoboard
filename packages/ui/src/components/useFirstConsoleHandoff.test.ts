// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { useFirstConsoleHandoff } from "./useFirstConsoleHandoff";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Inputs = { welcomeShown: boolean; creating: boolean };

/** Mounts the hook with a top bar button in the document and returns that button, a way to
 * re-render with other inputs, and a way to open or close a modal that is not the new-console
 * dialog. */
function mountHandoff(first: Inputs) {
  const page = document.body.appendChild(document.createElement("div"));
  page.innerHTML = '<div data-region="topbar"><button>toggle</button></div><button id="other">other</button>';
  const container = page.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    page.remove();
  });
  function Probe({ inputs }: { inputs: Inputs }) {
    useFirstConsoleHandoff(inputs.welcomeShown, inputs.creating);
    return null;
  }
  const render = (inputs: Inputs) => act(() => root.render(createElement(Probe, { inputs })));
  render(first);
  return {
    render,
    topBar: page.querySelector<HTMLElement>("[data-region=topbar] button")!,
    other: page.querySelector<HTMLElement>("#other")!,
    setModal: (open: boolean) => {
      page.querySelector("[role=dialog]")?.remove();
      if (open) page.insertAdjacentHTML("beforeend", '<div role="dialog"></div>');
    },
  };
}

const shown = { welcomeShown: true, creating: false };
const creatingShown = { welcomeShown: true, creating: true };
const gone = { welcomeShown: false, creating: false };

it("owes nothing to a launch that already has consoles", () => {
  const { render } = mountHandoff(gone);
  render(gone);
  expect(document.activeElement).toBe(document.body);
});

it("pays once the dialog closes when the console arrives while it is still open", () => {
  const { render, setModal, topBar } = mountHandoff(shown);
  render(creatingShown);
  setModal(true);
  render({ welcomeShown: false, creating: true });
  expect(document.activeElement).toBe(document.body);
  setModal(false);
  render(gone);
  expect(document.activeElement).toBe(topBar);
});

it("pays at once when the console arrives after the dialog has closed", () => {
  const { render, topBar } = mountHandoff(shown);
  render(creatingShown);
  render(shown);
  render(gone);
  expect(document.activeElement).toBe(topBar);
});

it("leaves focus alone for a console this window did not start making", () => {
  const { render } = mountHandoff(shown);
  render(gone);
  expect(document.activeElement).toBe(document.body);
});

it("holds the debt while a modal other than the new-console dialog is open", () => {
  const { render, setModal, topBar } = mountHandoff(shown);
  render(creatingShown);
  setModal(true);
  render({ welcomeShown: false, creating: false });
  expect(document.activeElement).toBe(document.body);
  setModal(false);
  render(gone);
  expect(document.activeElement).toBe(topBar);
});

it("drops an unpaid debt when the welcome screen comes back", () => {
  const { render, setModal } = mountHandoff(shown);
  render(creatingShown);
  setModal(true);
  render({ welcomeShown: false, creating: true });
  render(creatingShown);
  setModal(false);
  render(shown);
  expect(document.activeElement).toBe(document.body);
});

it("leaves focus alone when it is not on the body as the debt comes due", () => {
  const { render, other } = mountHandoff(shown);
  render(creatingShown);
  other.focus();
  render(gone);
  expect(document.activeElement).toBe(other);
});

it("pays only once", () => {
  const { render, setModal, topBar } = mountHandoff(shown);
  render(creatingShown);
  render(gone);
  topBar.blur();
  setModal(true);
  render({ welcomeShown: false, creating: true });
  render(gone);
  expect(document.activeElement).toBe(document.body);
});
