// @vitest-environment jsdom
import { act, useRef } from "react";
import { afterEach, describe, expect, it, onTestFinished, vi } from "vitest";

import type { FileDragEvent, PlatformAdapter } from "../platform";
import { BROWSER_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import type { TerminalController } from "./TerminalController";
import { useTerminalFileDrop } from "./useTerminalFileDrop";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  vi.restoreAllMocks();
});

/** The part of a controller the hook reads and calls. */
function fakeController(session: string | null = "s-1") {
  // `null` is a terminal that cannot take input.
  return { inputSessionId: session ?? undefined, pasteDroppedPaths: vi.fn(), dropFiles: vi.fn() };
}
type FakeController = ReturnType<typeof fakeController>;

/** A terminal view with the element xterm mounts in, as `TerminalSurface` and the controller's container make them. */
function Terminal({ controller, uploads, id }: { controller: FakeController; uploads: boolean; id: string }) {
  const controllerRef = useRef(controller as unknown as TerminalController);
  const containerRef = useRef<HTMLDivElement>(null);
  useTerminalFileDrop({ controllerRef, containerRef, uploads });
  return (
    <div data-terminal-surface="" data-testid={id}>
      <div ref={containerRef}>
        <span data-testid={`${id}-inner`} />
      </div>
    </div>
  );
}

/** The shell's file drop as a platform has it: `send` is a drag the shell reports to every listener. */
function nativeDrop() {
  const handlers = new Set<(event: FileDragEvent) => void>();
  const stop = vi.fn();
  const fileDrop = {
    listen: vi.fn((handler: (event: FileDragEvent) => void) => {
      handlers.add(handler);
      return () => {
        stop();
        handlers.delete(handler);
      };
    }),
  };
  return { fileDrop, stop, listening: () => handlers.size, send: (event: FileDragEvent) => act(() => handlers.forEach((handler) => handler(event))) };
}

/** jsdom has no hit-testing: `elementFromPoint` answers `at`'s element for the point it names, and records its calls. */
function hitTesting(at: () => Element | null) {
  const elementFromPoint = vi.fn((_x: number, _y: number) => at());
  Object.defineProperty(document, "elementFromPoint", { configurable: true, value: elementFromPoint });
  onTestFinished(() => {
    Reflect.deleteProperty(document, "elementFromPoint");
  });
  return elementFromPoint;
}

const find = (container: HTMLElement, id: string) => container.querySelector<HTMLElement>(`[data-testid="${id}"]`)!;
const marked = (view: HTMLElement) => view.hasAttribute("data-file-drop");

function mountTerminals(platform: Partial<PlatformAdapter>, terminals: { id: string; controller: FakeController; uploads: boolean }[]) {
  return mountHost(
    <>
      {terminals.map(({ id, controller, uploads }) => (
        <Terminal key={id} id={id} controller={controller} uploads={uploads} />
      ))}
    </>,
    { platform: { kind: "browser", host: BROWSER_HOST, ...platform } },
  );
}

describe("files dropped on a terminal where the platform reports their paths", () => {
  const over = (x: number, y: number): FileDragEvent => ({ kind: "over", position: { x, y } });
  const drop = (x: number, y: number, paths: string[]): FileDragEvent => ({ kind: "drop", position: { x, y }, paths });

  function setup(options: { controller?: FakeController; uploads?: boolean } = {}) {
    const native = nativeDrop();
    const controller = options.controller ?? fakeController();
    const container = mountTerminals({ fileDrop: native.fileDrop }, [{ id: "t", controller, uploads: options.uploads ?? true }]);
    const view = find(container, "t");
    const point = hitTesting(() => view.querySelector("span"));
    return { native, controller, container, view, point };
  }

  it("marks the terminal under the pointer while files are dragged over it, and clears the mark on leave", () => {
    const { native, view, point } = setup();
    native.send(over(30, 40));
    expect(point).toHaveBeenLastCalledWith(30, 40);
    expect(marked(view)).toBe(true);
    native.send({ kind: "leave" });
    expect(marked(view)).toBe(false);
  });

  it("hit-tests the position as the platform gave it, in CSS pixels", () => {
    const { native, point } = setup();
    native.send(over(12.5, 99.25));
    expect(point).toHaveBeenLastCalledWith(12.5, 99.25);
  });

  it("pastes the paths of a drop on the terminal, as one paste to its session, and clears the mark", () => {
    const { native, controller, view } = setup();
    native.send(over(1, 1));
    native.send(drop(1, 1, ["/Users/u/a.txt", "/Users/u/some folder"]));
    expect(controller.pasteDroppedPaths).toHaveBeenCalledExactlyOnceWith(["/Users/u/a.txt", "/Users/u/some folder"], "s-1");
    expect(marked(view)).toBe(false);
  });

  it("takes a drop on a terminal that takes no uploads too, as a shell is", () => {
    const { native, controller } = setup({ uploads: false });
    native.send(drop(1, 1, ["/a"]));
    expect(controller.pasteDroppedPaths).toHaveBeenCalledExactlyOnceWith(["/a"], "s-1");
  });

  it("does nothing for a drop that is not on the terminal", () => {
    const { native, controller, view, point } = setup();
    point.mockImplementation(() => document.body);
    native.send(over(1, 1));
    expect(marked(view)).toBe(false);
    native.send(drop(1, 1, ["/a"]));
    expect(controller.pasteDroppedPaths).not.toHaveBeenCalled();
  });

  it("does nothing for a drop where nothing is under the pointer", () => {
    const { native, controller, view, point } = setup();
    point.mockImplementation(() => null);
    native.send(over(1, 1));
    native.send(drop(1, 1, ["/a"]));
    expect(marked(view)).toBe(false);
    expect(controller.pasteDroppedPaths).not.toHaveBeenCalled();
  });

  it("does nothing for a terminal that cannot take input", () => {
    const { native, controller, view } = setup({ controller: fakeController(null) });
    native.send(over(1, 1));
    expect(marked(view)).toBe(false);
    native.send(drop(1, 1, ["/a"]));
    expect(controller.pasteDroppedPaths).not.toHaveBeenCalled();
  });

  it("does nothing for a covered terminal", () => {
    const { native, controller, view } = setup();
    view.setAttribute("data-covered", "");
    native.send(over(1, 1));
    expect(marked(view)).toBe(false);
    native.send(drop(1, 1, ["/a"]));
    expect(controller.pasteDroppedPaths).not.toHaveBeenCalled();
  });

  it("pastes to the session that takes input at the time of the drop", () => {
    const { native, controller } = setup();
    controller.inputSessionId = "s-2";
    native.send(drop(1, 1, ["/a"]));
    expect(controller.pasteDroppedPaths).toHaveBeenCalledExactlyOnceWith(["/a"], "s-2");
  });

  it("pastes nothing for a drop that carries no paths", () => {
    const { native, controller } = setup();
    native.send(drop(1, 1, []));
    expect(controller.pasteDroppedPaths).not.toHaveBeenCalled();
  });

  it("follows the pointer from one terminal to another, marking and pasting to only the one under it", () => {
    const native = nativeDrop();
    const first = fakeController("s-1");
    const second = fakeController("s-2");
    const container = mountTerminals({ fileDrop: native.fileDrop }, [
      { id: "a", controller: first, uploads: true },
      { id: "b", controller: second, uploads: false },
    ]);
    let under = "a-inner";
    hitTesting(() => find(container, under));
    native.send(over(1, 1));
    expect([marked(find(container, "a")), marked(find(container, "b"))]).toEqual([true, false]);
    under = "b-inner";
    native.send(over(2, 2));
    expect([marked(find(container, "a")), marked(find(container, "b"))]).toEqual([false, true]);
    native.send(drop(2, 2, ["/x"]));
    expect(first.pasteDroppedPaths).not.toHaveBeenCalled();
    expect(second.pasteDroppedPaths).toHaveBeenCalledExactlyOnceWith(["/x"], "s-2");
  });

  it("stops listening, and clears the mark, when the terminal goes", () => {
    const { native, container, view } = setup();
    native.send(over(1, 1));
    expect(marked(view)).toBe(true);
    expect(native.listening()).toBe(1);
    container.render(<></>);
    expect(native.stop).toHaveBeenCalledOnce();
    expect(native.listening()).toBe(0);
    expect(marked(view)).toBe(false);
  });

  it("takes no part in the page's own drag events", () => {
    const { view } = setup();
    const event = new Event("dragover", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "dataTransfer", { value: { types: ["Files"], dropEffect: "none" } });
    view.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    expect(marked(view)).toBe(false);
  });
});

describe("files dropped on a terminal where the page receives them", () => {
  type Item = { kind: string; getAsFile: () => File | null; webkitGetAsEntry?: () => { isDirectory: boolean } | null };

  function setup(options: { controller?: FakeController; uploads?: boolean } = {}) {
    const controller = options.controller ?? fakeController();
    const container = mountTerminals({}, [{ id: "t", controller, uploads: options.uploads ?? true }]);
    const view = find(container, "t");
    return { controller, container, view, inner: find(container, "t-inner") };
  }

  /** A drag event of `view`, as a browser raises it; `dataTransfer` is what it carries. */
  function drag(target: Element, type: string, { types = ["Files"], items = [] as Item[], relatedTarget = null as Node | null } = {}) {
    const event = new Event(type, { bubbles: true, cancelable: true });
    const dataTransfer = { types, items, dropEffect: "none" };
    Object.defineProperty(event, "dataTransfer", { value: dataTransfer });
    Object.defineProperty(event, "relatedTarget", { value: relatedTarget });
    act(() => void target.dispatchEvent(event));
    return { event, dataTransfer };
  }

  const fileItem = (file: File, isDirectory = false): Item => ({ kind: "file", getAsFile: () => file, webkitGetAsEntry: () => ({ isDirectory }) });

  it.each(["dragenter", "dragover"])("accepts a file drag over an agent session's terminal on %s: it is not refused, the drop effect is copy and the terminal is marked", (type) => {
    const { view, inner } = setup();
    const { event, dataTransfer } = drag(inner, type);
    expect(event.defaultPrevented).toBe(true);
    expect(dataTransfer.dropEffect).toBe("copy");
    expect(marked(view)).toBe(true);
  });

  it.each(["dragenter", "dragover"])("refuses a file drag over a terminal that takes no uploads on %s", (type) => {
    const { view, inner } = setup({ uploads: false });
    const { event, dataTransfer } = drag(inner, type);
    expect(event.defaultPrevented).toBe(false);
    expect(dataTransfer.dropEffect).not.toBe("copy");
    expect(marked(view)).toBe(false);
  });

  it("does not take a drag that carries no files", () => {
    const { view, inner } = setup();
    const { event, dataTransfer } = drag(inner, "dragover", { types: ["text/plain"] });
    expect(event.defaultPrevented).toBe(false);
    expect(dataTransfer.dropEffect).not.toBe("copy");
    expect(marked(view)).toBe(false);
  });

  it("does not take a drag over a terminal that cannot take input", () => {
    const { view, inner } = setup({ controller: fakeController(null) });
    const { event } = drag(inner, "dragover");
    expect(event.defaultPrevented).toBe(false);
    expect(marked(view)).toBe(false);
  });

  describe("the mark while a drag crosses the view's elements", () => {
    it("is cleared when the drag leaves the view", () => {
      const { view } = setup();
      drag(view, "dragenter");
      drag(view, "dragover");
      expect(marked(view)).toBe(true);
      drag(view, "dragleave");
      expect(marked(view)).toBe(false);
    });

    it("stays while the drag moves into an element inside the view, which enters the new element before it leaves the old one", () => {
      const { view, inner } = setup();
      drag(view, "dragenter");
      drag(inner, "dragenter");
      drag(view, "dragleave");
      drag(inner, "dragover");
      expect(marked(view)).toBe(true);
    });

    it("goes only when every element entered has been left", () => {
      const { view, inner } = setup();
      drag(view, "dragenter");
      drag(inner, "dragenter");
      drag(view, "dragleave");
      expect(marked(view)).toBe(true);
      drag(inner, "dragleave");
      expect(marked(view)).toBe(false);
    });

    it("is cleared by a cancelled drag, whose dragleave has no dragenter of its own left to match", () => {
      const { view } = setup();
      drag(view, "dragover");
      expect(marked(view)).toBe(true);
      drag(view, "dragleave");
      expect(marked(view)).toBe(false);
    });

    it("is cleared by a drop, and the count starts again for the next drag", () => {
      const { view, inner } = setup();
      drag(view, "dragenter");
      drag(inner, "dragenter");
      drag(inner, "drop", { items: [fileItem(new File(["a"], "a.txt"))] });
      expect(marked(view)).toBe(false);
      drag(view, "dragenter");
      expect(marked(view)).toBe(true);
      drag(view, "dragleave");
      expect(marked(view)).toBe(false);
    });

    it("is left alone by a drag that carries no files", () => {
      const { view } = setup();
      drag(view, "dragenter");
      expect(marked(view)).toBe(true);
      drag(view, "dragleave", { types: ["text/plain"] });
      expect(marked(view)).toBe(true);
    });

    it("is not moved by the enters of a drag that carries no files", () => {
      const { view } = setup();
      drag(view, "dragenter", { types: ["text/plain"] });
      drag(view, "dragenter");
      drag(view, "dragleave");
      expect(marked(view)).toBe(false);
    });
  });

  it("hands the files of a drop to the controller with the session it was dropped on, and clears the mark", () => {
    const { controller, view, inner } = setup();
    const first = new File(["a"], "a.txt");
    const second = new File([""], "docs");
    drag(inner, "dragover");
    const { event } = drag(inner, "drop", { items: [fileItem(first), fileItem(second, true), { kind: "string", getAsFile: () => null }] });
    expect(event.defaultPrevented).toBe(true);
    expect(marked(view)).toBe(false);
    expect(controller.dropFiles).toHaveBeenCalledExactlyOnceWith(
      [
        { file: first, folder: false },
        { file: second, folder: true },
      ],
      "s-1",
    );
  });

  it("does nothing with a drop on a terminal that takes no uploads", () => {
    const { controller, inner } = setup({ uploads: false });
    const { event } = drag(inner, "drop", { items: [fileItem(new File(["a"], "a.txt"))] });
    expect(event.defaultPrevented).toBe(false);
    expect(controller.dropFiles).not.toHaveBeenCalled();
  });

  it("does nothing with a drop on a terminal that cannot take input", () => {
    const { controller, inner } = setup({ controller: fakeController(null) });
    const { event } = drag(inner, "drop", { items: [fileItem(new File(["a"], "a.txt"))] });
    expect(event.defaultPrevented).toBe(false);
    expect(controller.dropFiles).not.toHaveBeenCalled();
  });

  it("does nothing with a drop that carries no files", () => {
    const { controller, inner } = setup();
    drag(inner, "drop", { types: ["text/plain"], items: [{ kind: "string", getAsFile: () => null }] });
    expect(controller.dropFiles).not.toHaveBeenCalled();
  });

  describe("under a cover", () => {
    /** The terminal view as `TerminalSurface` marks it while a cover hides the terminal. */
    const cover = (view: HTMLElement) => view.setAttribute("data-covered", "");

    it("refuses a file drag, marking nothing", () => {
      const { view, inner } = setup();
      cover(view);
      const { event, dataTransfer } = drag(inner, "dragover");
      expect(event.defaultPrevented).toBe(false);
      expect(dataTransfer.dropEffect).not.toBe("copy");
      expect(marked(view)).toBe(false);
    });

    it("clears the mark a drag had put on the terminal before the cover came up", () => {
      const { view, inner } = setup();
      drag(inner, "dragover");
      expect(marked(view)).toBe(true);
      cover(view);
      drag(inner, "dragover");
      expect(marked(view)).toBe(false);
    });

    it("takes no drop", () => {
      const { controller, view, inner } = setup();
      cover(view);
      const { event } = drag(inner, "drop", { items: [fileItem(new File(["a"], "a.txt"))] });
      expect(event.defaultPrevented).toBe(false);
      expect(controller.dropFiles).not.toHaveBeenCalled();
    });

    it("takes the drag again once the cover is gone", () => {
      const { view, inner } = setup();
      cover(view);
      view.removeAttribute("data-covered");
      expect(drag(inner, "dragover").event.defaultPrevented).toBe(true);
      expect(marked(view)).toBe(true);
    });
  });

  it("takes no drag once the terminal is gone", () => {
    const { container, view, inner } = setup();
    drag(inner, "dragover");
    expect(marked(view)).toBe(true);
    container.render(<></>);
    expect(marked(view)).toBe(false);
    const { event } = drag(inner, "dragover");
    expect(event.defaultPrevented).toBe(false);
  });
});
