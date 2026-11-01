// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { errorText } from "../errors/errorText";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { browseFailure, projectBrowseFailure } from "./browseError";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const records = { consoles: new Map(), projects: new Map(), sessions: new Map() };

function sourceUnavailable(path: string) {
  return new DaemonRequestError("unreadable", "source_unavailable", { path, detail: "denied" });
}

/** The plain text of a failure to show. */
const failed = (failure: ReturnType<typeof browseFailure>) => (failure.kind === "failed" ? errorText(failure.message) : "");

describe("browseFailure", () => {
  it("names the project's directory for the empty path, isolated once", () => {
    const failure = browseFailure(t, "en", sourceUnavailable(""), records, "~/project");
    expect(failure).toMatchObject({ kind: "failed" });
    expect(failed(failure)).toBe(`The folder ${isolate("~/project")} cannot be read: ${isolate("denied")}`);
  });

  it("keeps the shaped params beside the text, for a structured render of the same message", () => {
    const failure = browseFailure(t, "en", sourceUnavailable(""), records, "~/project");
    expect(failure.kind === "failed" && failure.message).toMatchObject({ code: "source_unavailable", params: { path: "~/project" } });
  });

  it("shows an ordinary path as it is, isolated once", () => {
    const failure = browseFailure(t, "en", sourceUnavailable("src/lib"), records, "~/project");
    expect(failed(failure)).toBe(`The folder ${isolate("src/lib")} cannot be read: ${isolate("denied")}`);
  });
});

describe("projectBrowseFailure", () => {
  const state = (path?: string) => ({ ...records, homeDir: "/Users/me", projects: new Map(path ? [["p1", { path }]] : []) }) as unknown as Parameters<typeof projectBrowseFailure>[3];

  it("names the project's directory, abbreviated to the home folder, for the empty path", () => {
    const failure = projectBrowseFailure(t, "en", sourceUnavailable(""), state("/Users/me/project"), "p1");
    expect(failed(failure)).toBe(`The folder ${isolate("~/project")} cannot be read: ${isolate("denied")}`);
  });

  it("leaves the empty path as it is for a project that is not known", () => {
    const failure = projectBrowseFailure(t, "en", sourceUnavailable(""), state(), "p1");
    expect(failure.kind === "failed" && failure.message).toMatchObject({ params: { path: "" } });
  });
});
