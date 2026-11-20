import { describe, expect, it } from "vitest";

import { isolate } from "../i18n/isolate";
import { droppedFileEntries, droppedFilesParams, droppedFilesText, isOwnSentence, type DroppedFileEntry } from "./droppedFileEntries";

const folder: DroppedFileEntry = { name: "docs", code: "dropped_file_folder", params: { name: "docs" }, message: "docs is a folder" };
const daemon: DroppedFileEntry = { name: "a.txt", code: "session_not_running", params: {}, message: "This session is not running." };

describe("the params of a refused-files alert", () => {
  it("carry the entries as one JSON param, which reads back as they were", () => {
    const entries: DroppedFileEntry[] = [folder, { ...daemon, name: 'odd, "name"\n.txt' }];
    const params = droppedFilesParams(entries);
    expect(Object.keys(params)).toEqual(["files"]);
    expect(droppedFileEntries("dropped_files_refused", params)).toEqual(entries);
  });

  it("keep a name with a comma whole", () => {
    const entries = [{ ...folder, name: "a,b.txt" }, { ...folder, name: "c.txt" }];
    expect(droppedFileEntries("dropped_files_refused", droppedFilesParams(entries))?.map((entry) => entry.name)).toEqual(["a,b.txt", "c.txt"]);
  });
});

describe("the entries of a failure", () => {
  const valid = JSON.stringify([folder]);

  it("are none for any other code", () => {
    expect(droppedFileEntries("dropped_file_folder", { files: valid })).toBeUndefined();
    expect(droppedFileEntries("session_not_running", { files: valid })).toBeUndefined();
  });

  it.each([
    ["no files param", {}],
    ["an empty files param", { files: "" }],
    ["text that is not JSON", { files: "{not json" }],
    ["JSON that is not an array", { files: JSON.stringify({ name: "a" }) }],
    ["an empty array", { files: "[]" }],
    ["an entry that is null", { files: JSON.stringify([null]) }],
    ["an entry with no name", { files: JSON.stringify([{ code: "x", params: {}, message: "m" }]) }],
    ["an entry with no code", { files: JSON.stringify([{ name: "a", params: {}, message: "m" }]) }],
    ["an entry with no message", { files: JSON.stringify([{ name: "a", code: "x", params: {} }]) }],
    ["an entry with no params", { files: JSON.stringify([{ name: "a", code: "x", message: "m" }]) }],
    ["one bad entry among good ones", { files: JSON.stringify([folder, { name: 1 }]) }],
  ])("are none for %s", (_name, params) => {
    expect(droppedFileEntries("dropped_files_refused", params)).toBeUndefined();
  });

  it("are the listed entries, in order", () => {
    expect(droppedFileEntries("dropped_files_refused", { files: JSON.stringify([folder, daemon]) })).toEqual([folder, daemon]);
  });
});

describe("whether an entry is worded by the window", () => {
  it("is so for a failure the window refused before sending", () => {
    for (const code of ["dropped_file_folder", "dropped_file_empty", "dropped_file_too_large", "dropped_file_images_only", "dropped_file_unreadable"]) {
      expect(isOwnSentence({ ...folder, code })).toBe(true);
    }
  });

  it("is not so for a failure of the daemon", () => {
    expect(isOwnSentence(daemon)).toBe(false);
  });

  it("is not so for any other code, though the window words it", () => {
    expect(isOwnSentence({ ...daemon, code: "unexpected_error" })).toBe(false);
    expect(isOwnSentence({ ...daemon, code: "dropped_files_refused" })).toBe(false);
  });
});

describe("the entries in plain text", () => {
  it("say a file the window refused in its own sentence, with the name isolated, one line to a file", () => {
    const text = droppedFilesText("en", [
      folder,
      { name: "empty.txt", code: "dropped_file_empty", params: { name: "empty.txt" }, message: "" },
      { name: "big.zip", code: "dropped_file_too_large", params: { name: "big.zip", limit_mib: "3" }, message: "" },
    ]);
    expect(text.split("\n")).toEqual([
      `${isolate("docs")} is a folder, and only files can be attached.`,
      `${isolate("empty.txt")} is empty.`,
      `${isolate("big.zip")} is larger than 3 MiB.`,
    ]);
  });

  it("says the limit of an older daemon's images-only refusal in MiB", () => {
    const text = droppedFilesText("en", [{ name: "n.txt", code: "dropped_file_images_only", params: { name: "n.txt", limit_mib: "10" }, message: "" }]);
    expect(text).toContain(isolate("n.txt"));
    expect(text).toContain("10 MiB");
    expect(text).toContain("too old");
  });

  it("says a file the daemon failed by its name and then the failure's message", () => {
    const [line] = droppedFilesText("en", [daemon]).split("\n");
    expect(line.startsWith(isolate("a.txt"))).toBe(true);
    expect(line.indexOf("a.txt")).toBeLessThan(line.indexOf("This session is not running."));
  });

  it("says a file that is not the window's own refusal with the reason it is given, in place of the failure's message", () => {
    const [line] = droppedFilesText("en", [daemon], () => "The reason, worded.").split("\n");
    expect(line).toBe(`${isolate("a.txt")} could not be attached. The reason, worded.`);
  });

  it("follows the language", () => {
    const en = droppedFilesText("en", [folder]);
    const zh = droppedFilesText("zh-CN", [folder]);
    expect(zh).not.toBe(en);
    expect(zh).toContain(isolate("docs"));
  });
});
