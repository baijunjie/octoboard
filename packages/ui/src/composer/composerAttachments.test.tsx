// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { sessionOf, snapshotState } from "../fixtures/builders";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { t } from "../i18n/language";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import type { FileDragEvent, PlatformAdapter } from "../platform";
import type { ClientRequestBody } from "../protocol";
import type { Daemon } from "../store";
import { click } from "../test/dom";
import { MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { composerDraft, setComposerDraft } from "./composerDrafts";
import { sessionComposerEnabled } from "./composerSetting";
import { SessionComposer } from "./SessionComposer";
import { daemonError } from "../test/daemonError";
import type { ViewedFile } from "../viewer/viewedFile";

/** Waits until every attachment of `session`'s draft has its path (the uploads are over) and `count` are held. */
async function attached(store: Daemon["store"], count: number): Promise<void> {
  await act(async () => {
    await vi.waitFor(() => {
      const { attachments } = composerDraft(store, "s");
      expect(attachments).toHaveLength(count);
      expect(attachments.every((attachment) => attachment.path !== undefined)).toBe(true);
    });
  });
}

function setUp({ platform, takesImages = true, refuseSave = false, onViewFile }: { platform?: PlatformAdapter; takesImages?: boolean; refuseSave?: boolean; onViewFile?: (file: ViewedFile, returnFocus?: () => boolean) => void } = {}) {
  const sent: ClientRequestBody[] = [];
  const session = sessionOf("s", "c", "p", "Work", "idle");
  const state = snapshotState({ sessions: [session], messageQueues: {} });
  const providers = state.providers!.map((provider) => (provider.id === session.agent ? { ...provider, capabilities: { ...provider.capabilities, message_images: takesImages } } : provider));
  const daemon = testDaemon({
    state: { ...state, providers },
    request: async (body) => {
      sent.push(body);
      switch (body.type) {
        case "save_pasted_image": return { type: "pasted_image_saved", id: null, path: "/home/u/.octoboard/run/s/pasted/paste-1.png" };
        case "create_attachment_upload": return { type: "attachment_upload_created", id: null, upload: "up-1" };
        case "save_attachment":
          if (refuseSave) throw daemonError("limit_exceeded", { limit: "attachment_upload_bytes", max: "104857600" });
          return { type: "attachment_saved", id: null, path: "/home/u/.octoboard/run/s/attachments/notes.txt" };
        case "create_queued_message": return { type: "queued_message_created", id: null, message: "new" };
        default: return { type: "ack", id: null };
      }
    },
  });
  const container = mountHost(<SessionComposer session={session} onViewFile={onViewFile} />, { daemon, platform: platform ?? { kind: "browser", host: MAC_HOST } });
  const field = () => container.querySelector<HTMLTextAreaElement>("textarea")!;
  return { sent, daemon, container, field };
}

function paste(field: HTMLTextAreaElement, files: File[]) {
  const event = new Event("paste", { bubbles: true, cancelable: true }) as ClipboardEvent;
  Object.defineProperty(event, "clipboardData", { value: { files } });
  act(() => {
    field.dispatchEvent(event);
  });
}

beforeEach(() => sessionComposerEnabled.set(true));
afterEach(() => {
  sessionComposerEnabled.set(false);
  acknowledgeAllAlerts();
});

describe("SessionComposer attachments", () => {
  it("attaches a pasted image as an image of the message where the agent takes images, and another file as a file", async () => {
    const { sent, daemon, field, container } = setUp();
    paste(field(), [new File([new Uint8Array([137, 80, 78, 71])], "shot.png", { type: "image/png" }), new File(["hello"], "notes.txt", { type: "text/plain" })]);
    await attached(daemon.store, 2);
    expect(sent.map((body) => body.type)).toEqual(["save_pasted_image", "create_attachment_upload", "write_attachment_part", "save_attachment"]);
    expect(composerDraft(daemon.store, "s").attachments.map(({ name, image, path }) => ({ name, image, path }))).toEqual([
      { name: "shot.png", image: true, path: "/home/u/.octoboard/run/s/pasted/paste-1.png" },
      { name: "notes.txt", image: false, path: "/home/u/.octoboard/run/s/attachments/notes.txt" },
    ]);
    const send = [...container.querySelectorAll("button")].find((b) => b.textContent === "Send")!;
    await click(send);
    expect(sent[sent.length - 1]).toEqual({
      type: "create_queued_message",
      session: "s",
      text: "",
      images: ["/home/u/.octoboard/run/s/pasted/paste-1.png"],
      files: ["/home/u/.octoboard/run/s/attachments/notes.txt"],
      mode: "now",
    });
  });

  it("attaches an image as a file where the agent takes no images", async () => {
    const { sent, daemon, field } = setUp({ takesImages: false });
    paste(field(), [new File([new Uint8Array([137, 80, 78, 71])], "shot.png", { type: "image/png" })]);
    await attached(daemon.store, 1);
    expect(sent.map((body) => body.type)).not.toContain("save_pasted_image");
    expect(composerDraft(daemon.store, "s").attachments[0]?.image).toBe(false);
  });

  it("gives a file dropped in the desktop application's own window by its own path", () => {
    let listener: ((event: FileDragEvent) => void) | undefined;
    const platform: PlatformAdapter = { kind: "browser", host: MAC_HOST, fileDrop: { listen: (handler) => { listener = handler; return () => {}; } } };
    const { daemon, container } = setUp({ platform });
    const section = container.querySelector("section")!;
    const elementFromPoint = document.elementFromPoint;
    document.elementFromPoint = () => section;
    try {
      act(() => listener!({ kind: "drop", position: { x: 1, y: 1 }, paths: ["/Users/u/Desktop/report.pdf"] }));
    } finally {
      document.elementFromPoint = elementFromPoint;
    }
    expect(composerDraft(daemon.store, "s").attachments.map(({ name, image, path }) => ({ name, image, path }))).toEqual([
      { name: "report.pdf", image: false, path: "/Users/u/Desktop/report.pdf" },
    ]);
  });

  it("removes an attachment", async () => {
    const { daemon, container, field } = setUp();
    paste(field(), [new File(["hello"], "notes.txt", { type: "text/plain" })]);
    await attached(daemon.store, 1);
    await click(container.querySelector("[data-attachment] button"));
    expect(composerDraft(daemon.store, "s").attachments).toEqual([]);
  });

  it("removes the focused attachment with Delete, keeping focus in the group, and hands focus to the text area when the last goes", async () => {
    const { daemon, container, field } = setUp();
    paste(field(), [new File(["a"], "a.txt", { type: "text/plain" }), new File(["b"], "b.txt", { type: "text/plain" })]);
    await attached(daemon.store, 2);
    const tag = (name: string) => [...container.querySelectorAll<HTMLElement>("[data-attachment]")].find((element) => element.textContent?.includes(name))!;
    const press = (element: HTMLElement, key: string) => act(() => {
      element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
      element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true, cancelable: true }));
    });
    act(() => tag("a.txt").focus());
    press(tag("a.txt"), "Delete");
    expect(composerDraft(daemon.store, "s").attachments.map(({ name }) => name)).toEqual(["b.txt"]);
    expect(document.activeElement?.closest("[data-attachment]")?.textContent).toContain("b.txt");
    press(tag("b.txt"), "Delete");
    expect(composerDraft(daemon.store, "s").attachments).toEqual([]);
    expect(document.activeElement).toBe(field());
  });

  it("describes a tag that has its path from its first render, and keeps that once the keyboard is in use", () => {
    const { daemon, container } = setUp({ onViewFile: () => {} });
    // As a file dropped in the desktop window, or a message being edited, brings it: the path is there at once.
    act(() => setComposerDraft(daemon.store, "s", { text: "", attachments: [{ id: "a1", name: "report.pdf", image: false, path: "/Users/u/report.pdf" }] }));
    const row = () => container.querySelector<HTMLElement>('[data-attachment="a1"]')!.closest<HTMLElement>('[role="row"]') ?? container.querySelector<HTMLElement>('[data-attachment="a1"]')!;
    const description = () => (row().getAttribute("aria-describedby") ?? "").split(" ").map((id) => document.getElementById(id)?.textContent ?? "").join(" ");
    expect(description()).toContain("Opens in the file viewer");
    // A key press puts React Aria in keyboard modality, where it writes the tag's own description (how to remove it).
    act(() => {
      document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
      document.body.dispatchEvent(new KeyboardEvent("keyup", { key: "Tab", bubbles: true }));
    });
    act(() => row().focus());
    expect(description()).toContain("Press Delete to remove tag");
    expect(description()).toContain("Opens in the file viewer");
  });

  it("says that an attachment opens in the file viewer, and gives one still being sent no action, pointer or hover tint", async () => {
    const { daemon, container, field } = setUp({ onViewFile: () => {} });
    paste(field(), [new File(["hello"], "notes.txt", { type: "text/plain" })]);
    const tag = () => container.querySelector<HTMLElement>("[data-attachment]")!;
    const description = () => (tag().getAttribute("aria-describedby") ?? "").split(" ").map((id) => document.getElementById(id)?.textContent ?? "").join(" ");
    // While it is being sent.
    expect(description()).not.toContain("Opens in the file viewer");
    expect(tag().className).toContain("cursor-default");
    await attached(daemon.store, 1);
    expect(description()).toContain("Opens in the file viewer");
    expect(tag().className).not.toContain("cursor-default");
  });

  it("opens an attachment in the file viewer by its path, from the daemon's host, and gives focus back to it", async () => {
    const viewed: Array<{ file: unknown; returnFocus?: () => boolean }> = [];
    const { daemon, container, field } = setUp({ onViewFile: (file, returnFocus) => viewed.push({ file, returnFocus }) });
    paste(field(), [new File(["hello"], "notes.txt", { type: "text/plain" })]);
    await attached(daemon.store, 1);
    const tag = container.querySelector<HTMLElement>("[data-attachment]")!;
    await click(tag);
    expect(viewed.map(({ file }) => file)).toEqual([{ link: "/home/u/.octoboard/run/s/attachments/notes.txt", project: "p" }]);
    act(() => field().focus());
    expect(viewed[0]!.returnFocus!()).toBe(true);
    expect(document.activeElement).toBe(container.querySelector("[data-attachment]"));
  });

  /** A drop of `files` as the page reports it. */
  function drop(target: HTMLElement, files: Array<{ file: File; directory?: boolean }>, type = "drop") {
    const event = new Event(type, { bubbles: true, cancelable: true }) as DragEvent;
    const items = files.map(({ file, directory = false }) => ({ kind: "file", getAsFile: () => file, webkitGetAsEntry: () => ({ isDirectory: directory }) }));
    Object.defineProperty(event, "dataTransfer", { value: { types: ["Files"], items, dropEffect: "none" } });
    act(() => {
      target.dispatchEvent(event);
    });
    return event;
  }

  it("takes files dropped on the composer in a browser window, and marks the composer while a drag over it would be taken", async () => {
    const { daemon, container } = setUp();
    const section = container.querySelector("section")!;
    drop(section, [], "dragenter");
    expect(section.hasAttribute("data-file-drop")).toBe(true);
    drop(section, [], "dragleave");
    expect(section.hasAttribute("data-file-drop")).toBe(false);
    const event = drop(section, [{ file: new File(["hello"], "notes.txt", { type: "text/plain" }) }]);
    expect(event.defaultPrevented).toBe(true);
    await attached(daemon.store, 1);
    expect(composerDraft(daemon.store, "s").attachments[0]).toMatchObject({ name: "notes.txt", image: false, path: "/home/u/.octoboard/run/s/attachments/notes.txt" });
  });

  it("takes no drop while it is inert, behind the archive view, as the terminal takes none under its cover", () => {
    const { daemon, container } = setUp();
    const section = container.querySelector("section")!;
    container.setAttribute("inert", "");
    drop(section, [], "dragenter");
    expect(section.hasAttribute("data-file-drop")).toBe(false);
    const event = drop(section, [{ file: new File(["hello"], "notes.txt", { type: "text/plain" }) }]);
    expect(event.defaultPrevented).toBe(false);
    expect(composerDraft(daemon.store, "s").attachments).toEqual([]);
  });

  it("ignores a drag that carries no files", () => {
    const { container } = setUp();
    const section = container.querySelector("section")!;
    const event = new Event("dragenter", { bubbles: true, cancelable: true }) as DragEvent;
    Object.defineProperty(event, "dataTransfer", { value: { types: ["text/plain"], items: [] } });
    act(() => {
      section.dispatchEvent(event);
    });
    expect(event.defaultPrevented).toBe(false);
    expect(section.hasAttribute("data-file-drop")).toBe(false);
  });

  it("takes the files picked with the attach button", async () => {
    const { daemon, container, sent } = setUp();
    const input = container.querySelector<HTMLInputElement>('input[type="file"]')!;
    Object.defineProperty(input, "files", { configurable: true, value: [new File(["hello"], "notes.txt", { type: "text/plain" })] });
    await act(async () => {
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await attached(daemon.store, 1);
    expect(sent.map((body) => body.type)).toEqual(["create_attachment_upload", "write_attachment_part", "save_attachment"]);
  });

  it("leaves a file the daemon refused out of the message, and raises an alert for it", async () => {
    const { daemon, field } = setUp({ refuseSave: true });
    paste(field(), [new File(["hello"], "notes.txt", { type: "text/plain" })]);
    await act(async () => {
      await vi.waitFor(() => expect(errorAlerts.getSnapshot()).toHaveLength(1));
    });
    expect(errorAlerts.getSnapshot()[0]?.title).toBe(t("terminal.dropFileFailedTitle"));
    expect(composerDraft(daemon.store, "s").attachments).toEqual([]);
  });

  it("refuses a dropped folder, as a drop on the terminal does, attaching nothing", async () => {
    const { daemon, container, sent } = setUp();
    drop(container.querySelector("section")!, [{ file: new File([""], "docs"), directory: true }]);
    await act(async () => {
      await vi.waitFor(() => expect(errorAlerts.getSnapshot()).toHaveLength(1));
    });
    expect(composerDraft(daemon.store, "s").attachments).toEqual([]);
    expect(sent).toEqual([]);
  });

  it("cannot send while a file is still being sent, and can once it has arrived", async () => {
    let finish!: () => void;
    const gate = new Promise<void>((resolve) => (finish = resolve));
    const sent: ClientRequestBody[] = [];
    const session = sessionOf("s", "c", "p", "Work", "idle");
    const daemon = testDaemon({
      state: snapshotState({ sessions: [session], messageQueues: {} }),
      request: async (body) => {
        sent.push(body);
        if (body.type === "create_attachment_upload") return { type: "attachment_upload_created", id: null, upload: "up-1" };
        if (body.type === "save_attachment") {
          await gate;
          return { type: "attachment_saved", id: null, path: "/home/u/notes.txt" };
        }
        return { type: "ack", id: null };
      },
    });
    const container = mountHost(<SessionComposer session={session} />, { daemon, platform: { kind: "browser", host: MAC_HOST } });
    paste(container.querySelector("textarea")!, [new File(["hello"], "notes.txt", { type: "text/plain" })]);
    await act(async () => {
      await vi.waitFor(() => expect(sent.map((body) => body.type)).toContain("save_attachment"));
    });
    const send = () => [...container.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Send")!;
    expect(container.textContent).toContain("notes.txt");
    expect(send().disabled || send().getAttribute("aria-disabled") === "true").toBe(true);
    finish();
    await attached(daemon.store, 1);
    expect(send().disabled || send().getAttribute("aria-disabled") === "true").toBe(false);
  });
});
