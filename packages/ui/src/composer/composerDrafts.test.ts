// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import { sessionOf, snapshotState } from "../fixtures/builders";
import { testDaemon } from "../test/testDaemon";
import {
  appendToComposer,
  composerDraft,
  EMPTY_DRAFT,
  focusComposer,
  quoteIntoComposer,
  registerComposerEditor,
  setComposerDraft,
  updateComposerDraft,
} from "./composerDrafts";

function storeWith(...ids: string[]) {
  return testDaemon({ state: snapshotState({ sessions: ids.map((id) => sessionOf(id, "c", "p", id, "idle")), messageQueues: {} }) }).store;
}

describe("a session's draft", () => {
  it("is empty until something is held, and is kept per session", () => {
    const store = storeWith("a", "b");
    expect(composerDraft(store, "a")).toBe(EMPTY_DRAFT);
    setComposerDraft(store, "a", { text: "for a", attachments: [] });
    expect(composerDraft(store, "a").text).toBe("for a");
    expect(composerDraft(store, "b")).toBe(EMPTY_DRAFT);
  });

  it("is kept per daemon store, so another daemon starts with none", () => {
    const first = storeWith("a");
    const second = storeWith("a");
    setComposerDraft(first, "a", { text: "held", attachments: [] });
    expect(composerDraft(second, "a")).toBe(EMPTY_DRAFT);
  });

  it("is dropped once it holds no text, no attachment and no edit", () => {
    const store = storeWith("a");
    setComposerDraft(store, "a", { text: "x", attachments: [] });
    updateComposerDraft(store, "a", (draft) => ({ ...draft, text: "" }));
    expect(composerDraft(store, "a")).toBe(EMPTY_DRAFT);
  });

  it("is kept while it holds only an attachment", () => {
    const store = storeWith("a");
    const attachments = [{ id: "1", name: "a.txt", image: false, path: "/a.txt" }];
    setComposerDraft(store, "a", { text: "", attachments });
    expect(composerDraft(store, "a").attachments).toEqual(attachments);
  });

  it("is lost for a session the store no longer has, when another draft is set", () => {
    const store = storeWith("a", "b");
    setComposerDraft(store, "a", { text: "gone soon", attachments: [] });
    store.setState({ sessions: new Map([["b", sessionOf("b", "c", "p", "b", "idle")]]) });
    setComposerDraft(store, "b", { text: "kept", attachments: [] });
    expect(composerDraft(store, "a")).toBe(EMPTY_DRAFT);
    expect(composerDraft(store, "b").text).toBe("kept");
  });
});

describe("text put into the composer from elsewhere", () => {
  it("goes at the end of the draft on a line of its own, whether or not a composer is on screen", () => {
    const store = storeWith("a");
    appendToComposer(store, "a", "src/app.ts");
    expect(composerDraft(store, "a").text).toBe("src/app.ts");
    appendToComposer(store, "a", "README.md");
    expect(composerDraft(store, "a").text).toBe("src/app.ts\nREADME.md");
    setComposerDraft(store, "a", { text: "ends a line\n", attachments: [] });
    const editor = { focus: vi.fn() };
    registerComposerEditor(store, "a", editor);
    appendToComposer(store, "a", "next");
    expect(composerDraft(store, "a").text).toBe("ends a line\nnext");
    expect(editor.focus).not.toHaveBeenCalled();
  });

  it("reaches the composer on screen again only once the one that left lets go, and not after a newer one took its place", () => {
    const store = storeWith("a");
    const first = { focus: vi.fn() };
    const second = { focus: vi.fn() };
    const letGoFirst = registerComposerEditor(store, "a", first);
    registerComposerEditor(store, "a", second);
    letGoFirst();
    expect(focusComposer(store, "a")).toBe(true);
    expect(second.focus).toHaveBeenCalledOnce();
    expect(first.focus).not.toHaveBeenCalled();
  });
});

describe("text put into an empty-looking composer", () => {
  it.each(["", "\n", "  \n "])("goes in with no leading line break when the draft is %j", (text) => {
    const store = storeWith("a");
    setComposerDraft(store, "a", { text, attachments: [] });
    appendToComposer(store, "a", "src/app.ts");
    expect(composerDraft(store, "a").text).toBe("src/app.ts");
    setComposerDraft(store, "a", { text, attachments: [] });
    quoteIntoComposer(store, "a", "quoted");
    expect(composerDraft(store, "a").text).toBe("> quoted\n");
  });

  it("goes on a new line after text without a trailing line break, and directly after one with it", () => {
    const store = storeWith("a");
    setComposerDraft(store, "a", { text: "see", attachments: [] });
    quoteIntoComposer(store, "a", "quoted");
    expect(composerDraft(store, "a").text).toBe("see\n> quoted\n");
    setComposerDraft(store, "a", { text: "see\n", attachments: [] });
    quoteIntoComposer(store, "a", "quoted");
    expect(composerDraft(store, "a").text).toBe("see\n> quoted\n");
  });
});

describe("focusComposer", () => {
  it("says whether a composer was on screen to take focus", () => {
    const store = storeWith("a");
    expect(focusComposer(store, "a")).toBe(false);
    const editor = { focus: vi.fn() };
    const letGo = registerComposerEditor(store, "a", editor);
    expect(focusComposer(store, "a")).toBe(true);
    letGo();
    expect(focusComposer(store, "a")).toBe(false);
  });
});

describe("quoteIntoComposer", () => {
  it("puts the text at the end of the draft as a block quote on a line of its own, and nothing for text that quotes to nothing", () => {
    const store = storeWith("a");
    setComposerDraft(store, "a", { text: "see", attachments: [] });
    quoteIntoComposer(store, "a", "first\nsecond");
    expect(composerDraft(store, "a").text).toBe("see\n> first\n> second\n");
    quoteIntoComposer(store, "a", " \n ");
    expect(composerDraft(store, "a").text).toBe("see\n> first\n> second\n");
  });
});
