// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import type { PlatformAdapter } from "../platform";
import { BROWSER_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { useStrayFileDrops } from "./useStrayFileDrops";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Probe() {
  useStrayFileDrops();
  return null;
}

const FILE_DROP: NonNullable<PlatformAdapter["fileDrop"]> = { listen: () => () => {} };

/** A drag event as a browser raises it on `target`, carrying `types`. */
function drag(type: "dragover" | "drop", types: string[], target: EventTarget = document.body) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  const dataTransfer = { types, dropEffect: "copy" };
  Object.defineProperty(event, "dataTransfer", { value: dataTransfer });
  target.dispatchEvent(event);
  return { event, dataTransfer };
}

const mount = (platform: Partial<PlatformAdapter> = {}) => mountHost(<Probe />, { platform: { kind: "browser", host: BROWSER_HOST, ...platform } });

describe("a file dragged over the page where it receives the drag itself", () => {
  it("is refused with drop effect none, and its drop is prevented from navigating the page", () => {
    mount();
    const over = drag("dragover", ["Files"]);
    expect(over.event.defaultPrevented).toBe(true);
    expect(over.dataTransfer.dropEffect).toBe("none");
    expect(drag("drop", ["Files"]).event.defaultPrevented).toBe(true);
  });

  it("is left to what is under the pointer when that took the drag", () => {
    mount();
    const target = document.body.appendChild(document.createElement("div"));
    target.addEventListener("dragover", (event) => {
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    });
    const over = drag("dragover", ["Files"], target);
    expect(over.event.defaultPrevented).toBe(true);
    expect(over.dataTransfer.dropEffect).toBe("copy");
    target.remove();
  });

  it("is not touched when what is dragged carries no files", () => {
    mount();
    const over = drag("dragover", ["text/plain"]);
    expect(over.event.defaultPrevented).toBe(false);
    expect(over.dataTransfer.dropEffect).toBe("copy");
    expect(drag("drop", ["text/plain"]).event.defaultPrevented).toBe(false);
  });

  it("is no longer handled once the page is unmounted", () => {
    const container = mount();
    container.render(<></>);
    expect(drag("dragover", ["Files"]).event.defaultPrevented).toBe(false);
    expect(drag("drop", ["Files"]).event.defaultPrevented).toBe(false);
  });
});

describe("where the platform takes the drag itself", () => {
  it("leaves every drag and drop alone", () => {
    mount({ fileDrop: FILE_DROP });
    expect(drag("dragover", ["Files"]).event.defaultPrevented).toBe(false);
    expect(drag("drop", ["Files"]).event.defaultPrevented).toBe(false);
  });
});
