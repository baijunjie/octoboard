// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { isLoneSlashCommand, messageExcerpt, queuedMode, SEND_MODES, sendModeOf } from "./messageQueue";

describe("the send modes", () => {
  it("lists now, when idle and manual", () => {
    expect([...SEND_MODES]).toEqual(["now", "when_idle", "manual"]);
  });

  it("reads a session's default mode, and a mode this client does not know as none, left to the daemon", () => {
    expect(sendModeOf("when_idle")).toBe("when_idle");
    expect(sendModeOf("manual")).toBe("manual");
    expect(sendModeOf("now")).toBe("now");
    expect(sendModeOf(undefined)).toBeUndefined();
    expect(sendModeOf("sometime")).toBeUndefined();
  });

  it("reads a queued message's mode as the one that never sends by itself where it is not one this client knows", () => {
    expect(queuedMode({ mode: "when_idle" })).toBe("when_idle");
    expect(queuedMode({ mode: "manual" })).toBe("manual");
    expect(queuedMode({ mode: "at_noon" as never })).toBe("manual");
  });
});

describe("isLoneSlashCommand", () => {
  it.each([
    ["/compact", true],
    ["  /compact  ", true],
    ["/compact keep the plan", true],
    ["/model\topus", true],
    ["/compact\nand more", false],
    ["look at /compact", false],
    ["/", false],
    ["// comment", false],
    ["/ space first", false],
    ["/usr/bin/env foo", false],
    ["/tmp/notes.txt", false],
    ["/review src/app.ts", true],
    ["", false],
  ])("reads %j as a command: %s", (text, expected) => {
    expect(isLoneSlashCommand(text)).toBe(expected);
  });
});

describe("messageExcerpt", () => {
  it("is the text on one line, with runs of whitespace as one space", () => {
    expect(messageExcerpt({ text: "  first\n\n  second\tthird  " })).toBe("first second third");
    expect(messageExcerpt({ text: "" })).toBe("");
  });
});
