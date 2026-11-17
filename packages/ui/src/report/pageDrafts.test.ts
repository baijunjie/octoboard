// @vitest-environment jsdom
import { expect, it } from "vitest";

import { sessionOf } from "../fixtures/builders";
import { createStateStore } from "../store";
import { heldDrafts, holdDrafts } from "./pageDrafts";

/** A store that has the sessions named, as a daemon's has the sessions it serves. */
function storeWith(...ids: string[]) {
  return createStateStore({ sessions: new Map(ids.map((id) => [id, sessionOf(id, "console", undefined, id, "idle")])) });
}

it("holds nothing for a session that has had nothing held", () => {
  expect(heldDrafts(storeWith("a"), "a")).toEqual({});
});

it("returns what was held for a session, and keeps each session's own", () => {
  const store = storeWith("a", "b");
  holdDrafts(store, "a", { "page-1": { note: "first" } });
  holdDrafts(store, "b", { "page-2": { note: "second" } });
  expect(heldDrafts(store, "a")).toEqual({ "page-1": { note: "first" } });
  expect(heldDrafts(store, "b")).toEqual({ "page-2": { note: "second" } });
});

it("replaces what was held for a session with what is held next", () => {
  const store = storeWith("a");
  holdDrafts(store, "a", { "page-1": { note: "first" }, "page-2": { note: "other" } });
  holdDrafts(store, "a", { "page-1": { note: "edited" } });
  expect(heldDrafts(store, "a")).toEqual({ "page-1": { note: "edited" } });
});

it("lets go of a session's drafts when it is given an empty set, leaving the other sessions' alone", () => {
  const store = storeWith("a", "b");
  holdDrafts(store, "a", { "page-1": { note: "first" } });
  holdDrafts(store, "b", { "page-2": { note: "second" } });
  holdDrafts(store, "a", {});
  expect(heldDrafts(store, "a")).toEqual({});
  expect(heldDrafts(store, "b")).toEqual({ "page-2": { note: "second" } });
});

it("holds nothing for a session when it is given an empty set first", () => {
  const store = storeWith("a");
  holdDrafts(store, "a", {});
  expect(heldDrafts(store, "a")).toEqual({});
});

it("keeps each store's drafts apart: another store sees none", () => {
  const first = storeWith("a");
  const second = storeWith("a");
  holdDrafts(first, "a", { "page-1": { note: "first" } });
  expect(heldDrafts(second, "a")).toEqual({});
  holdDrafts(second, "a", { "page-1": { note: "second" } });
  expect(heldDrafts(first, "a")).toEqual({ "page-1": { note: "first" } });
  expect(heldDrafts(second, "a")).toEqual({ "page-1": { note: "second" } });
});

it("lets go of the drafts of a session the store no longer has when any session's drafts are next held", () => {
  const store = storeWith("a", "b");
  holdDrafts(store, "a", { "page-1": { note: "first" } });
  holdDrafts(store, "b", { "page-2": { note: "second" } });

  store.setState({ sessions: new Map([["b", sessionOf("b", "console", undefined, "b", "idle")]]) });
  holdDrafts(store, "b", { "page-2": { note: "edited" } });
  expect(heldDrafts(store, "b")).toEqual({ "page-2": { note: "edited" } });

  // It stays gone: a session that comes back under the same id does not find what was held before it left.
  store.setState({ sessions: new Map(["a", "b"].map((id) => [id, sessionOf(id, "console", undefined, id, "idle")])) });
  expect(heldDrafts(store, "a")).toEqual({});
});

it("lets go of a gone session's drafts also when the drafts held next are an empty set", () => {
  const store = storeWith("a", "b");
  holdDrafts(store, "a", { "page-1": { note: "first" } });
  holdDrafts(store, "b", { "page-2": { note: "second" } });
  store.setState({ sessions: new Map([["b", sessionOf("b", "console", undefined, "b", "idle")]]) });
  holdDrafts(store, "b", {});
  store.setState({ sessions: new Map(["a", "b"].map((id) => [id, sessionOf(id, "console", undefined, id, "idle")])) });
  expect(heldDrafts(store, "a")).toEqual({});
});
