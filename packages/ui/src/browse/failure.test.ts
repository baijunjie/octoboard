// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { CodedError } from "../errors/codedError";
import { errorText } from "../errors/errorText";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { browseFailure, fileTooLarge, projectBrowseFailure } from "./failure";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const records = { consoles: new Map(), projects: new Map(), sessions: new Map() };

function sourceUnavailable(path: string) {
  return new CodedError("unreadable", "source_unavailable", { path, detail: "denied" });
}

/** The plain text of a failure to show. */
const failed = (failure: ReturnType<typeof browseFailure>) => (failure.kind === "failed" ? errorText(failure.message) : "");

describe("fileTooLarge", () => {
  it.each<{ name: string; params: Record<string, string>; expected: { size?: number } }>([
    { name: "the size the daemon gave", params: { limit: "file_bytes", max: "4194304", size: "9000000" }, expected: { size: 9000000 } },
    { name: "a size of zero", params: { limit: "file_bytes", max: "4194304", size: "0" }, expected: { size: 0 } },
    { name: "no size", params: { limit: "file_bytes", max: "4194304" }, expected: {} },
    { name: "a size that is not a number", params: { limit: "file_bytes", max: "4194304", size: "many" }, expected: {} },
  ])("is $expected for a read past the file budget with $name", ({ params, expected }) => {
    const found = fileTooLarge(new CodedError("too large", "limit_exceeded", params));
    expect(found).toEqual(expected);
    if (!("size" in expected)) expect(found).not.toHaveProperty("size");
  });

  it.each([
    { name: "another limit", error: new CodedError("busy", "limit_exceeded", { limit: "reply_bytes", max: "1", size: "2" }) },
    { name: "another code", error: new CodedError("gone", "file_not_found", { limit: "file_bytes", size: "2" }) },
    { name: "an error that is not coded", error: new Error("boom") },
    { name: "a value that is not an error", error: "file_bytes" },
    { name: "nothing", error: undefined },
  ])("is nothing for $name", ({ error }) => {
    expect(fileTooLarge(error)).toBeUndefined();
  });
});

describe("browseFailure", () => {
  it("marks a read past the file budget as too large, with the size where the daemon said it, and no other limit", () => {
    const over = (params: Record<string, string>) => browseFailure(t, "en", new CodedError("too large", "limit_exceeded", params), records);
    expect(over({ limit: "file_bytes", max: "4194304", size: "9000000" })).toMatchObject({ kind: "failed", tooLarge: { size: 9000000 } });
    expect(over({ limit: "file_bytes", max: "4194304" })).toMatchObject({ kind: "failed", tooLarge: {} });
    expect(over({ limit: "file_bytes", max: "4194304" })).not.toHaveProperty("tooLarge.size");
    for (const limit of ["pending_requests", "patch_bytes", "reply_bytes"]) expect(over({ limit, max: "1", size: "2" }), limit).not.toHaveProperty("tooLarge");
  });

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

describe("browseFailure by code", () => {
  const sort = (err: unknown) => browseFailure(t, "en", err, records).kind;

  it("takes a request lost with the connection for disconnected, and a timed-out one for a failure to show", () => {
    expect(sort(new CodedError("closed", "connection_closed", {}))).toBe("disconnected");
    expect(sort(new CodedError("timed out", "request_timed_out", {}))).toBe("failed");
  });

  it("drops a superseded request, retries a changed one, and shows an error that is not the daemon's", () => {
    expect(sort(new CodedError("", "request_superseded", {}))).toBe("superseded");
    expect(sort(new CodedError("", "source_changed", {}))).toBe("changed");
    expect(browseFailure(t, "en", new Error("boom"), records)).toMatchObject({ kind: "failed", code: "unexpected_error" });
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
