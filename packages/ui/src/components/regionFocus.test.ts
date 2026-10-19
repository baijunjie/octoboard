// @vitest-environment jsdom
import { expect, it, onTestFinished, vi } from "vitest";

import { focusAfterLoss } from "./regionFocus";

function addToBody(html: string): HTMLElement {
  const holder = document.body.appendChild(document.createElement("div"));
  holder.innerHTML = html;
  onTestFinished(() => {
    holder.remove();
    (document.activeElement as HTMLElement | null)?.blur();
  });
  return holder;
}

it("hands focus lost to the dialog still open first, never to a region behind it", () => {
  addToBody('<div data-region="sidebar"><button id="row">row</button></div><div role="dialog" id="dlg" tabindex="-1">dialog</div>');
  const terminal = vi.fn();
  focusAfterLoss(terminal);
  expect(document.activeElement?.id).toBe("dlg");
  expect(terminal).not.toHaveBeenCalled();
});

it("leaves focus where focusTerminal put it, inside the terminal, though a sidebar exists", () => {
  addToBody('<div data-region="sidebar"><button id="row">row</button></div><div data-region="terminal"><textarea id="term"></textarea></div>');
  focusAfterLoss(() => document.getElementById("term")!.focus());
  expect(document.activeElement?.id).toBe("term");
});

it("falls to the sidebar when the welcome screen or an empty terminal declines focus from the body, as they do", () => {
  addToBody('<div data-region="sidebar"><button id="row">row</button></div><div data-region="welcome"><button id="new">new</button></div>');
  // Their own method keeps the guard: nothing is pulled onto a control from `<body>`.
  focusAfterLoss(() => {
    if (document.activeElement !== document.body) document.getElementById("new")!.focus();
  });
  expect(document.activeElement?.id).toBe("row");
});

it("goes to the sidebar, then the rail, when the terminal takes no focus", () => {
  addToBody('<div data-region="rail"><button id="rail-button">rail</button></div><div data-region="sidebar"><button id="row" disabled>row</button><button id="second">second</button></div>');
  focusAfterLoss(() => {});
  expect(document.activeElement?.id).toBe("second");
  (document.activeElement as HTMLElement).blur();
  document.getElementById("second")!.setAttribute("disabled", "");
  focusAfterLoss(() => {});
  expect(document.activeElement?.id).toBe("rail-button");
});
