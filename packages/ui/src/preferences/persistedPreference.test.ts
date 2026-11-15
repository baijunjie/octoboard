// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";

import { createPersistedPreference, setPreferenceDaemonIdentity, storageKeyFor } from "./persistedPreference";

/** A text preference that falls back to `fallback`. */
const parse = (fallback?: string) => (raw: string | null): string | undefined => raw ?? fallback;
const serialize = (value: string | undefined): string | null => value ?? null;

/** Opens the page as the entry does for a window: the local daemon's, or with `remote` the window of the daemon
 * `identity` names, whose identity the entry hands in. */
function openWindow(remote?: { identity: string }): void {
  setPreferenceDaemonIdentity(remote?.identity);
}

/** What another window storing `value` at `key` sends this one. */
function storedElsewhere(key: string, value: string | null): void {
  window.dispatchEvent(new StorageEvent("storage", { key, newValue: value, storageArea: localStorage }));
}

afterEach(() => {
  localStorage.clear();
  setPreferenceDaemonIdentity(undefined);
});

describe("a daemon's preference", () => {
  it("is kept under the bare key in the local daemon's window, so what it kept before stays", () => {
    openWindow();
    localStorage.setItem("p.console", "c1");
    const console = createPersistedPreference<string | undefined>("p.console", parse(), serialize, { scope: "daemon" });
    expect(console.get()).toBe("c1");
    console.set("c2");
    expect(localStorage.getItem("p.console")).toBe("c2");
  });

  it("is kept under the daemon's identity in another daemon's window, which neither changes nor removes the local one's", () => {
    localStorage.setItem("p.console", "local");
    openWindow({ identity: "me@host" });
    const console = createPersistedPreference<string | undefined>("p.console", parse(), serialize, { scope: "daemon" });
    expect(console.get()).toBeUndefined();
    console.set("remote");
    expect(localStorage.getItem("p.console@me@host")).toBe("remote");
    console.set(undefined);
    expect(localStorage.getItem("p.console@me@host")).toBeNull();
    expect(localStorage.getItem("p.console")).toBe("local");
  });

  it("takes no change another window stores, which is another daemon's", () => {
    openWindow();
    const console = createPersistedPreference<string | undefined>("p.console", parse("none"), serialize, { scope: "daemon" });
    storedElsewhere("p.console", "elsewhere");
    expect(console.get()).toBe("none");
  });

  it("starts from the application's value when it inherits and the daemon has none of its own yet", () => {
    localStorage.setItem("p.width", "300");
    openWindow({ identity: "me@host" });
    expect(createPersistedPreference<string | undefined>("p.width", parse("default"), serialize, { scope: "daemon", inherit: true }).get()).toBe("300");
    expect(createPersistedPreference<string | undefined>("p.width", parse("default"), serialize, { scope: "daemon" }).get()).toBe("default");
    localStorage.setItem("p.width@me@host", "500");
    expect(createPersistedPreference<string | undefined>("p.width", parse("default"), serialize, { scope: "daemon", inherit: true }).get()).toBe("500");
  });

  it("is kept under the bare key until the entry hands in an identity", () => {
    expect(storageKeyFor("p.console", "daemon")).toBe("p.console");
    expect(storageKeyFor("p.console", "daemon", "me@host")).toBe("p.console@me@host");
  });
});

describe("an application preference", () => {
  it("is kept under the bare key in every window", () => {
    openWindow({ identity: "me@host" });
    const size = createPersistedPreference<string | undefined>("p.size", parse("12"), serialize);
    size.set("14");
    expect(localStorage.getItem("p.size")).toBe("14");
  });

  it("takes up a change another window stores, and tells its subscribers", () => {
    openWindow({ identity: "me@host" });
    const size = createPersistedPreference<string | undefined>("p.size", parse("12"), serialize);
    const calls: string[] = [];
    size.subscribe(() => calls.push(`changed to ${size.get()}`));
    storedElsewhere("p.other", "99");
    storedElsewhere("p.size", "16");
    expect(size.get()).toBe("16");
    storedElsewhere("p.size", null);
    expect(size.get()).toBe("12");
    expect(calls).toEqual(["changed to 16", "changed to 12"]);
  });
});
