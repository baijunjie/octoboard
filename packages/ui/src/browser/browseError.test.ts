// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { browseFailure } from "./browseError";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const records = { consoles: new Map(), projects: new Map(), sessions: new Map() };

function sourceUnavailable(path: string) {
  return new DaemonRequestError("unreadable", "source_unavailable", { path, detail: "denied" });
}

describe("browseFailure", () => {
  it("names the project's directory for the empty path, isolated once", () => {
    const failure = browseFailure(t, "en", sourceUnavailable(""), records, "~/project");
    expect(failure).toMatchObject({
      kind: "failed",
      message: `The folder “${isolate("~/project")}” cannot be read: ${isolate("denied")}`,
    });
  });

  it("shows an ordinary path as it is, isolated once", () => {
    const failure = browseFailure(t, "en", sourceUnavailable("src/lib"), records, "~/project");
    expect(failure).toMatchObject({ message: `The folder “${isolate("src/lib")}” cannot be read: ${isolate("denied")}` });
  });
});
