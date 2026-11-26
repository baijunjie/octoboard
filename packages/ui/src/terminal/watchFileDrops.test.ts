// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import type { FileDragEvent, FileDropCapability } from "../platform";
import type { DroppedFile } from "./fileDrag";
import { FILE_DROP_TARGET, watchFileDrops } from "./watchFileDrops";

let element: HTMLElement;
let inside: HTMLElement;
let outside: HTMLElement;

beforeEach(() => {
  element = document.body.appendChild(document.createElement("div"));
  inside = element.appendChild(document.createElement("span"));
  outside = document.body.appendChild(document.createElement("div"));
});
afterEach(() => {
  element.remove();
  outside.remove();
});

const marked = () => element.hasAttribute(FILE_DROP_TARGET);

/** The shell's file drop as a platform has it; `send` is a drag the shell reports. */
function nativeDrop() {
  let handler: ((event: FileDragEvent) => void) | undefined;
  const stop = vi.fn();
  const fileDrop: FileDropCapability = {
    listen: (listener) => {
      handler = listener;
      return stop;
    },
  };
  return { fileDrop, stop, send: (event: FileDragEvent) => handler?.(event) };
}

/** jsdom has no hit-testing: `elementFromPoint` answers `at` for any point. */
function hitTesting(at: Element | null) {
  Object.defineProperty(document, "elementFromPoint", { configurable: true, value: () => at });
  onTestFinished(() => {
    Reflect.deleteProperty(document, "elementFromPoint");
  });
}

describe("where the platform reports the paths of dragged files", () => {
  const POSITION = { x: 5, y: 5 };

  function watch(accepting = () => true) {
    const native = nativeDrop();
    const paths = vi.fn();
    const files = vi.fn();
    const unwatch = watchFileDrops(element, native.fileDrop, { accepting, paths, files });
    return { ...native, paths, files, unwatch };
  }

  it("marks the element while a drag is over it, finding it by the position the shell gives, and unmarks it as the drag moves away or leaves", () => {
    const { send } = watch();
    hitTesting(inside);
    send({ kind: "over", position: POSITION });
    expect(marked()).toBe(true);
    hitTesting(outside);
    send({ kind: "over", position: POSITION });
    expect(marked()).toBe(false);
    hitTesting(inside);
    send({ kind: "over", position: POSITION });
    send({ kind: "leave" });
    expect(marked()).toBe(false);
  });

  it("gives the paths of a drop on the element, and nothing for a drop elsewhere or one that carries none, leaving no mark", () => {
    const { send, paths } = watch();
    hitTesting(outside);
    send({ kind: "drop", position: POSITION, paths: ["/a/b.txt"] });
    hitTesting(inside);
    send({ kind: "drop", position: POSITION, paths: [] });
    expect(paths).not.toHaveBeenCalled();
    send({ kind: "drop", position: POSITION, paths: ["/a/b.txt", "/c.png"] });
    expect(paths).toHaveBeenCalledExactlyOnceWith(["/a/b.txt", "/c.png"]);
    expect(marked()).toBe(false);
  });

  it("takes nothing while it is not accepting, as under a cover or while inert", () => {
    let accepting = false;
    const { send, paths } = watch(() => accepting);
    hitTesting(inside);
    send({ kind: "over", position: POSITION });
    expect(marked()).toBe(false);
    send({ kind: "drop", position: POSITION, paths: ["/a.txt"] });
    expect(paths).not.toHaveBeenCalled();
    accepting = true;
    send({ kind: "over", position: POSITION });
    expect(marked()).toBe(true);
    send({ kind: "drop", position: POSITION, paths: ["/a.txt"] });
    expect(paths).toHaveBeenCalledExactlyOnceWith(["/a.txt"]);
  });

  it("stops listening and drops the mark when it is stopped", () => {
    const { send, stop, unwatch } = watch();
    hitTesting(inside);
    send({ kind: "over", position: POSITION });
    expect(marked()).toBe(true);
    unwatch();
    expect(stop).toHaveBeenCalledOnce();
    expect(marked()).toBe(false);
  });

  it("takes no drag of the page itself, which the shell's own drop replaces", () => {
    const { files } = watch();
    const event = dragEvent("drop", ["Files"], [{ kind: "file", getAsFile: () => new File(["x"], "x.txt") }]);
    element.dispatchEvent(event);
    expect(files).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });
});

/** A drag event as a browser raises it, `dataTransfer` being what it carries. */
function dragEvent(type: string, types: string[], items: unknown[] = []) {
  const event = new Event(type, { bubbles: true, cancelable: true }) as DragEvent;
  Object.defineProperty(event, "dataTransfer", { value: { types, items, dropEffect: "none" } });
  return event;
}

describe("where the page receives the drag itself", () => {
  function watch(accepting = () => true) {
    const files = vi.fn<(files: DroppedFile[]) => void>();
    const unwatch = watchFileDrops(element, undefined, { accepting, paths: vi.fn(), files });
    onTestFinished(unwatch);
    return { files, unwatch };
  }
  const file = (name: string) => ({ kind: "file", getAsFile: () => new File(["x"], name), webkitGetAsEntry: () => ({ isDirectory: false }) });

  it("marks the element while a drag of files is over it, asking for a copy, and not for a drag of anything else", () => {
    watch();
    const over = dragEvent("dragover", ["Files"]);
    inside.dispatchEvent(over);
    expect(over.defaultPrevented).toBe(true);
    expect((over.dataTransfer as { dropEffect: string }).dropEffect).toBe("copy");
    expect(marked()).toBe(true);
    element.removeAttribute(FILE_DROP_TARGET);
    const text = dragEvent("dragover", ["text/plain"]);
    inside.dispatchEvent(text);
    expect(text.defaultPrevented).toBe(false);
    expect(marked()).toBe(false);
  });

  it("keeps the mark until the drag has left every element it crossed, counting the enters and leaves", () => {
    watch();
    const send = (type: string, target: Element) => target.dispatchEvent(dragEvent(type, ["Files"]));
    send("dragenter", element);
    send("dragenter", inside);
    send("dragleave", element);
    expect(marked()).toBe(true);
    send("dragleave", inside);
    expect(marked()).toBe(false);
  });

  it("gives the files read from a drop, preventing the page from opening them, and unmarks the element", () => {
    const { files } = watch();
    inside.dispatchEvent(dragEvent("dragenter", ["Files"]));
    expect(marked()).toBe(true);
    const drop = dragEvent("drop", ["Files"], [file("a.txt"), { kind: "string", getAsFile: () => null }, file("b.txt")]);
    inside.dispatchEvent(drop);
    expect(drop.defaultPrevented).toBe(true);
    expect(files).toHaveBeenCalledOnce();
    expect(files.mock.calls[0][0].map((dropped) => [dropped.file.name, dropped.folder])).toEqual([["a.txt", false], ["b.txt", false]]);
    expect(marked()).toBe(false);
    // The count starts again with the next drag: one enter and one leave end it.
    inside.dispatchEvent(dragEvent("dragenter", ["Files"]));
    inside.dispatchEvent(dragEvent("dragleave", ["Files"]));
    expect(marked()).toBe(false);
  });

  it("reports a dropped folder as one, for the caller to refuse", () => {
    const { files } = watch();
    inside.dispatchEvent(dragEvent("drop", ["Files"], [{ kind: "file", getAsFile: () => new File([""], "docs"), webkitGetAsEntry: () => ({ isDirectory: true }) }]));
    expect(files.mock.calls[0][0].map((dropped) => dropped.folder)).toEqual([true]);
  });

  it("takes nothing while it is not accepting, leaving the drop to the page", () => {
    const { files } = watch(() => false);
    const enter = dragEvent("dragenter", ["Files"]);
    inside.dispatchEvent(enter);
    expect(enter.defaultPrevented).toBe(false);
    expect(marked()).toBe(false);
    const drop = dragEvent("drop", ["Files"], [file("a.txt")]);
    inside.dispatchEvent(drop);
    expect(drop.defaultPrevented).toBe(false);
    expect(files).not.toHaveBeenCalled();
  });

  it("takes nothing of a drag with no files in it", () => {
    const { files } = watch();
    const drop = dragEvent("drop", ["text/plain"], [{ kind: "string", getAsFile: () => null }]);
    inside.dispatchEvent(drop);
    expect(drop.defaultPrevented).toBe(false);
    expect(files).not.toHaveBeenCalled();
  });

  it("takes no such drag where it is given no `files`, and stops taking them, with the mark, once stopped", () => {
    const unwatchless = watchFileDrops(element, undefined, { accepting: () => true, paths: vi.fn() });
    const event = dragEvent("dragenter", ["Files"]);
    inside.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    expect(marked()).toBe(false);
    unwatchless();
    const { files, unwatch } = watch();
    inside.dispatchEvent(dragEvent("dragenter", ["Files"]));
    expect(marked()).toBe(true);
    unwatch();
    expect(marked()).toBe(false);
    inside.dispatchEvent(dragEvent("drop", ["Files"], [file("a.txt")]));
    expect(files).not.toHaveBeenCalled();
  });
});
