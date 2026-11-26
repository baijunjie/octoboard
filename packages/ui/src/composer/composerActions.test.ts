// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { queuedMessage, sessionOf, snapshotState } from "../fixtures/builders";
import { t } from "../i18n/language";
import type { ClientRequestBody } from "../protocol";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { testDaemon } from "../test/testDaemon";
import {
  attachLocalPaths,
  canSendDraft,
  chooseEditedMode,
  editQueuedMessage,
  endEdit,
  keepEditAsNew,
  MAX_MESSAGE_IMAGES,
  messageOfDraft,
  messageText,
  moveQueued,
  removeAttachment,
  saveEdit,
  sendDraft,
  sendQueuedNow,
  setQueuedMode,
  setSendMode,
  withdrawQueued,
} from "./composerActions";
import { composerDraft, setComposerDraft, updateComposerDraft, type ComposerAttachment, type ComposerDraft } from "./composerDrafts";
import { daemonError } from "../test/daemonError";
import { flushPromises } from "../test/clock";

afterEach(() => acknowledgeAllAlerts());

const file = (name: string, path?: string): ComposerAttachment => ({ id: `id-${name}`, name, image: false, ...(path !== undefined && { path }) });
const image = (name: string, path?: string): ComposerAttachment => ({ id: `id-${name}`, name, image: true, ...(path !== undefined && { path }) });
const draftOf = (text: string, attachments: ComposerAttachment[] = []): ComposerDraft => ({ text, attachments });

function setUp(reply: (body: ClientRequestBody) => void = () => {}) {
  const sent: ClientRequestBody[] = [];
  const daemon = testDaemon({
    state: snapshotState({ sessions: [sessionOf("s", "c", "p", "Work", "idle")], messageQueues: {} }),
    request: async (body) => {
      sent.push(body);
      reply(body);
      if (body.type === "create_queued_message") return { type: "queued_message_created", id: null, message: "new" };
      return { type: "ack", id: null };
    },
  });
  const context = { store: daemon.store, request: daemon.request, t };
  return { sent, store: daemon.store, context };
}

const refusing = () => {
  throw daemonError("limit_exceeded", { limit: "queued_messages", max: "100" });
};

describe("the message a draft stands for", () => {
  it("drops leading blank lines and trailing whitespace, and keeps the first line's indentation", () => {
    expect(messageText("\n \n  indented\nsecond  \n\n")).toBe("  indented\nsecond");
    expect(messageText(" \n\t")).toBe("");
  });

  it("has something to send with text or an attachment, and nothing while a file is still being sent", () => {
    expect(canSendDraft(draftOf(""))).toBe(false);
    expect(canSendDraft(draftOf(" \n "))).toBe(false);
    expect(canSendDraft(draftOf("hi"))).toBe(true);
    expect(canSendDraft(draftOf("", [file("a.txt", "/a.txt")]))).toBe(true);
    expect(canSendDraft(draftOf("hi", [file("a.txt")]))).toBe(false);
  });

  it("keeps images and files apart", () => {
    const { images, files } = messageOfDraft(draftOf("look", [image("a.png", "/a.png"), file("b.txt", "/b.txt"), image("c.png", "/c.png")]), true);
    expect(images).toEqual(["/a.png", "/c.png"]);
    expect(files).toEqual(["/b.txt"]);
  });

  it("is a command only for one slash command on one line with nothing attached, sent trimmed", () => {
    expect(messageOfDraft(draftOf("  /compact keep the plan \n"), true)).toMatchObject({ text: "/compact keep the plan", command: true });
    expect(messageOfDraft(draftOf("/compact\nmore"), true).command).toBe(false);
    expect(messageOfDraft(draftOf("/compact", [file("a.txt", "/a.txt")]), true).command).toBe(false);
    expect(messageOfDraft(draftOf("explain /compact"), true).command).toBe(false);
  });

  it("sends a slash command as text to an agent that takes no slash commands", () => {
    expect(messageOfDraft(draftOf("/compact"), false)).toMatchObject({ text: "/compact", command: false });
  });

  it("holds back a message with more images than one message takes", () => {
    const images = (count: number) => Array.from({ length: count }, (_, i) => image(`${i}.png`, `/${i}.png`));
    expect(canSendDraft(draftOf("", images(MAX_MESSAGE_IMAGES)))).toBe(true);
    expect(canSendDraft(draftOf("", images(MAX_MESSAGE_IMAGES + 1)))).toBe(false);
  });
});

describe("sending a draft", () => {
  it("queues the message in the mode given, with only the lists that hold something, and clears the draft once taken", async () => {
    const { sent, store, context } = setUp();
    setComposerDraft(store, "s", draftOf("read this", [file("b.txt", "/b.txt")]));
    expect(await sendDraft(context, "s", "manual", true)).toBe(true);
    expect(sent).toEqual([{ type: "create_queued_message", session: "s", text: "read this", files: ["/b.txt"], mode: "manual" }]);
    expect(composerDraft(store, "s").text).toBe("");
    expect(composerDraft(store, "s").attachments).toEqual([]);
  });

  it("sends an attachment with no text", async () => {
    const { sent, store, context } = setUp();
    setComposerDraft(store, "s", draftOf("", [image("a.png", "/a.png")]));
    await sendDraft(context, "s", "now", true);
    expect(sent).toEqual([{ type: "create_queued_message", session: "s", text: "", images: ["/a.png"], mode: "now" }]);
  });

  it("sends nothing for an empty draft, one with a file still being sent, or one that edits a queued message", async () => {
    const { sent, store, context } = setUp();
    expect(await sendDraft(context, "s", "now", true)).toBe(false);
    setComposerDraft(store, "s", draftOf("hi", [file("a.txt")]));
    expect(await sendDraft(context, "s", "now", true)).toBe(false);
    setComposerDraft(store, "s", { ...draftOf("hi"), editing: { message: "m1", images: [], files: [], set: draftOf("") } });
    expect(await sendDraft(context, "s", "now", true)).toBe(false);
    expect(sent).toEqual([]);
  });

  it("keeps the draft and raises an alert when the daemon refuses it", async () => {
    const { store, context } = setUp(refusing);
    setComposerDraft(store, "s", draftOf("keep me"));
    expect(await sendDraft(context, "s", "now", true)).toBe(false);
    expect(composerDraft(store, "s").text).toBe("keep me");
    expect(errorAlerts.getSnapshot()[0]?.title).toBe(t("composer.sendFailedTitle"));
  });

  it("leaves the mode to the daemon where the session's is one this client does not know", async () => {
    const { sent, store, context } = setUp();
    setComposerDraft(store, "s", draftOf("hello"));
    await sendDraft(context, "s", undefined, true);
    expect(sent).toEqual([{ type: "create_queued_message", session: "s", text: "hello" }]);
  });

  it("takes only what was sent out of the draft, keeping a file attached while the message was out", async () => {
    const { store, context } = setUp(() => updateComposerDraft(store, "s", (draft) => ({ ...draft, attachments: [...draft.attachments, image("late.png", "/late.png")] })));
    setComposerDraft(store, "s", draftOf("sent", [file("a.txt", "/a.txt")]));
    await sendDraft(context, "s", "now", true);
    expect(composerDraft(store, "s")).toEqual(draftOf("", [image("late.png", "/late.png")]));
  });

  it("keeps only what was quoted into the draft while the message was out, so nothing goes twice", async () => {
    const { store, context } = setUp(() => updateComposerDraft(store, "s", (draft) => ({ ...draft, text: `${draft.text}\n> quoted\n` })));
    setComposerDraft(store, "s", draftOf("hello"));
    await sendDraft(context, "s", "now", true);
    expect(composerDraft(store, "s").text).toBe("> quoted\n");
  });

  it("keeps what was typed while the message was out", async () => {
    const { store, context } = setUp(() => updateComposerDraft(store, "s", (draft) => ({ ...draft, text: "typed meanwhile" })));
    setComposerDraft(store, "s", draftOf("sent"));
    await sendDraft(context, "s", "now", true);
    expect(composerDraft(store, "s").text).toBe("typed meanwhile");
  });
});

describe("editing a queued message in the composer", () => {
  const queued = queuedMessage({ id: "m1", text: "queued text", mode: "manual", images: ["/home/u/a.png"], files: ["/home/u/b.txt"] });

  it("puts its text and attachments in the editor, naming them by their file names, and sets the new message aside", () => {
    const { store, context } = setUp();
    setComposerDraft(store, "s", draftOf("new message", [file("n.txt", "/n.txt")]));
    editQueuedMessage(context, "s", queued, "unix");
    const draft = composerDraft(store, "s");
    expect(draft.text).toBe("queued text");
    expect(draft.attachments.map(({ name, image, path }) => ({ name, image, path }))).toEqual([
      { name: "a.png", image: true, path: "/home/u/a.png" },
      { name: "b.txt", image: false, path: "/home/u/b.txt" },
    ]);
    expect(draft.editing?.message).toBe("m1");
  });

  it("brings the new message back when the edit ends, even after switching to another message", () => {
    const { store, context } = setUp();
    setComposerDraft(store, "s", draftOf("new message", [file("n.txt", "/n.txt")]));
    editQueuedMessage(context, "s", queued, "unix");
    editQueuedMessage(context, "s", queuedMessage({ id: "m2", text: "second", mode: "manual" }), "unix");
    endEdit(context, "s");
    expect(composerDraft(store, "s")).toEqual(draftOf("new message", [file("n.txt", "/n.txt")]));
  });

  it("ends an edit with nothing to bring back as an empty draft, and leaves a draft that is no edit alone", () => {
    const { store, context } = setUp();
    editQueuedMessage(context, "s", queued, "unix");
    endEdit(context, "s");
    expect(composerDraft(store, "s").text).toBe("");
    setComposerDraft(store, "s", draftOf("plain"));
    endEdit(context, "s");
    expect(composerDraft(store, "s").text).toBe("plain");
  });

  it("saves the text and the command flag, sending the lists only where they changed, and ends the edit", async () => {
    const { sent, store, context } = setUp();
    setComposerDraft(store, "s", draftOf("new message"));
    editQueuedMessage(context, "s", queued, "unix");
    updateComposerDraft(store, "s", (draft) => ({ ...draft, text: "better", attachments: draft.attachments.filter((a) => a.image) }));
    expect(await saveEdit(context, "s", true)).toBe(true);
    expect(sent).toEqual([{ type: "update_queued_message", session: "s", message: "m1", text: "better", command: false, files: [] }]);
    expect(composerDraft(store, "s")).toEqual(draftOf("new message"));
  });

  it("keeps the edit and raises an alert when the daemon refuses it", async () => {
    const { store, context } = setUp(refusing);
    editQueuedMessage(context, "s", queued, "unix");
    expect(await saveEdit(context, "s", true)).toBe(false);
    expect(composerDraft(store, "s").editing?.message).toBe("m1");
    expect(errorAlerts.getSnapshot()[0]?.title).toBe(t("queue.changeFailedTitle"));
  });

  it("saves nothing where no edit is under way or nothing is left to send", async () => {
    const { sent, store, context } = setUp();
    setComposerDraft(store, "s", draftOf("plain"));
    expect(await saveEdit(context, "s", true)).toBe(false);
    editQueuedMessage(context, "s", queuedMessage({ id: "m1", text: "x", mode: "manual" }), "unix");
    updateComposerDraft(store, "s", (draft) => ({ ...draft, text: "" }));
    expect(await saveEdit(context, "s", true)).toBe(false);
    expect(sent).toEqual([]);
  });

  it("keeps what the editor holds as a new message after the one it edited left the queue, after what was set aside", () => {
    const { store, context } = setUp();
    setComposerDraft(store, "s", draftOf("set aside", [file("n.txt", "/n.txt")]));
    editQueuedMessage(context, "s", queued, "unix");
    keepEditAsNew(store, "s");
    const draft = composerDraft(store, "s");
    expect(draft.editing).toBeUndefined();
    expect(draft.text).toBe("set aside\nqueued text");
    expect(draft.attachments.map(({ name }) => name)).toEqual(["n.txt", "a.png", "b.txt"]);
  });

  it("keeps only the editor's text where nothing was set aside", () => {
    const { store, context } = setUp();
    editQueuedMessage(context, "s", queued, "unix");
    keepEditAsNew(store, "s");
    expect(composerDraft(store, "s").text).toBe("queued text");
  });

  it("ends the edit once saved, keeping what was typed or attached while the save was out after the message set aside", async () => {
    const { store, context } = setUp((body) => {
      if (body.type === "update_queued_message") updateComposerDraft(store, "s", (draft) => ({ ...draft, text: "added meanwhile", attachments: [...draft.attachments, file("late.txt", "/late.txt")] }));
    });
    setComposerDraft(store, "s", draftOf("set aside"));
    editQueuedMessage(context, "s", queued, "unix");
    expect(await saveEdit(context, "s", true)).toBe(true);
    const draft = composerDraft(store, "s");
    expect(draft.editing).toBeUndefined();
    expect(draft.text).toBe("set aside\nadded meanwhile");
    expect(draft.attachments.map(({ name }) => name)).toEqual(["late.txt"]);
  });

  it("keeps only what was quoted while a save was out, after the message set aside", async () => {
    const { store, context } = setUp((body) => {
      if (body.type === "update_queued_message") updateComposerDraft(store, "s", (draft) => ({ ...draft, text: `${draft.text}\n> quoted` }));
    });
    setComposerDraft(store, "s", draftOf("set aside"));
    editQueuedMessage(context, "s", queued, "unix");
    await saveEdit(context, "s", true);
    expect(composerDraft(store, "s").text).toBe("set aside\n> quoted");
  });

  it("records a mode picked during the edit instead of sending it, and applies it as the edit ends", () => {
    const { sent, store, context } = setUp();
    const idle = queuedMessage({ id: "m1", text: "waits", mode: "when_idle" });
    editQueuedMessage(context, "s", idle, "unix");
    sent.length = 0;
    chooseEditedMode(store, "s", "manual");
    expect(sent).toEqual([]);
    endEdit(context, "s");
    // Manual chosen: it stays as the edit held it.
    expect(sent).toEqual([]);
    // A manual message given When idle during its edit goes on waiting for the agent once the edit ends.
    editQueuedMessage(context, "s", queued, "unix");
    chooseEditedMode(store, "s", "when_idle");
    expect(sent).toEqual([]);
    endEdit(context, "s");
    expect(sent).toEqual([{ type: "set_queued_message_mode", session: "s", message: "m1", mode: "when_idle" }]);
  });

  it("tells no refusal of holding back a message the daemon sent meanwhile, but tells any other", async () => {
    const gone = setUp((body) => {
      if (body.type === "set_queued_message_mode") throw daemonError("queued_message_not_found", { session: "s", message: body.message });
    });
    editQueuedMessage(gone.context, "s", queuedMessage({ id: "m1", text: "waits", mode: "when_idle" }), "unix");
    await flushPromises();
    expect(errorAlerts.getSnapshot()).toEqual([]);
    const refused = setUp((body) => {
      if (body.type === "set_queued_message_mode") throw daemonError("queued_message_sending", { session: "s", message: body.message });
    });
    editQueuedMessage(refused.context, "s", queuedMessage({ id: "m1", text: "waits", mode: "when_idle" }), "unix");
    await flushPromises();
    expect(errorAlerts.getSnapshot()[0]?.title).toBe(t("queue.changeFailedTitle"));
  });

  it("holds a message waiting for the agent to be idle back as manual while it is edited, and puts its mode back after", async () => {
    const { sent, store, context } = setUp();
    const idle = queuedMessage({ id: "m1", text: "waits", mode: "when_idle" });
    editQueuedMessage(context, "s", idle, "unix");
    expect(sent).toEqual([{ type: "set_queued_message_mode", session: "s", message: "m1", mode: "manual" }]);
    endEdit(context, "s");
    expect(sent[1]).toEqual({ type: "set_queued_message_mode", session: "s", message: "m1", mode: "when_idle" });
    sent.length = 0;
    editQueuedMessage(context, "s", idle, "unix");
    updateComposerDraft(store, "s", (draft) => ({ ...draft, text: "waits, changed" }));
    await saveEdit(context, "s", true);
    expect(sent.map((body) => [body.type, "mode" in body ? body.mode : undefined])).toEqual([
      ["set_queued_message_mode", "manual"],
      ["update_queued_message", undefined],
      ["set_queued_message_mode", "when_idle"],
    ]);
  });

  it("leaves a manual message's mode alone around its edit", () => {
    const { sent, context } = setUp();
    editQueuedMessage(context, "s", queued, "unix");
    endEdit(context, "s");
    expect(sent).toEqual([]);
  });
});

describe("attachments", () => {
  it("adds files given by their own paths, each named by its file name and as a file", () => {
    const { store } = setUp();
    attachLocalPaths(store, "s", ["/Users/u/Desktop/report.pdf", "/Users/u/shot.png"], "unix");
    expect(composerDraft(store, "s").attachments.map(({ name, image, path }) => ({ name, image, path }))).toEqual([
      { name: "report.pdf", image: false, path: "/Users/u/Desktop/report.pdf" },
      { name: "shot.png", image: false, path: "/Users/u/shot.png" },
    ]);
    const before = composerDraft(store, "s");
    attachLocalPaths(store, "s", [], "unix");
    expect(composerDraft(store, "s")).toBe(before);
  });

  it("names a file by a Windows path's last part where the host is Windows", () => {
    const { store } = setUp();
    attachLocalPaths(store, "s", ["C:\\Users\\u\\report.pdf"], "windows");
    expect(composerDraft(store, "s").attachments[0]?.name).toBe("report.pdf");
  });

  it("removes one by its id and leaves the rest", () => {
    const { store } = setUp();
    setComposerDraft(store, "s", draftOf("", [file("a.txt", "/a.txt"), file("b.txt", "/b.txt")]));
    removeAttachment(store, "s", "id-a.txt");
    expect(composerDraft(store, "s").attachments.map(({ name }) => name)).toEqual(["b.txt"]);
  });
});

describe("the requests on the queue", () => {
  it.each([
    ["sets the session's default mode", (c: ReturnType<typeof setUp>["context"]) => setSendMode(c, "s", "manual"), { type: "set_session_send_mode", session: "s", mode: "manual" }, "composer.modeFailedTitle"],
    ["sends a message now", (c: ReturnType<typeof setUp>["context"]) => sendQueuedNow(c, "s", "m1"), { type: "send_queued_message", session: "s", message: "m1" }, "composer.sendFailedTitle"],
    ["withdraws a message", (c: ReturnType<typeof setUp>["context"]) => withdrawQueued(c, "s", "m1"), { type: "withdraw_queued_message", session: "s", message: "m1" }, "queue.withdrawFailedTitle"],
    ["sets a message's mode", (c: ReturnType<typeof setUp>["context"]) => setQueuedMode(c, "s", "m1", "when_idle"), { type: "set_queued_message_mode", session: "s", message: "m1", mode: "when_idle" }, "queue.changeFailedTitle"],
    ["moves a message", (c: ReturnType<typeof setUp>["context"]) => moveQueued(c, "s", "m1", 2), { type: "move_queued_message", session: "s", message: "m1", position: 2 }, "queue.changeFailedTitle"],
  ] as const)("%s, and raises an alert titled for it when refused", async (_name, run, body, title) => {
    const ok = setUp();
    await run(ok.context);
    expect(ok.sent).toEqual([body]);
    expect(errorAlerts.getSnapshot()).toEqual([]);
    const failing = setUp(refusing);
    await run(failing.context);
    expect(errorAlerts.getSnapshot()[0]?.title).toBe(t(title));
  });
});
