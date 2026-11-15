import { describe, expect, it } from "vitest";

import { parseConnectionDetails } from "./connectionDetails";

describe("parseConnectionDetails", () => {
  it("reads every field octoboardd connection prints, decoding what it encoded", () => {
    expect(
      parseConnectionDetails(
        "octoboard+ssh://me@server.example:22?fingerprint=SHA256:ab%2Bc/d&octoboardd=/home/me/.local/bin/octoboardd&name=B%C3%BCro%20%26%20Co",
      ),
    ).toEqual({
      host: "server.example",
      port: 22,
      user: "me",
      fingerprint: "SHA256:ab+c/d",
      octoboarddPath: "/home/me/.local/bin/octoboardd",
      name: "Büro & Co",
    });
  });

  it("takes a bracketed IPv6 host with its zone, ignores surrounding space and a trailing slash, and defaults the port", () => {
    expect(parseConnectionDetails("  octoboard+ssh://me@[fe80::1%25eth0]:2222\n")).toEqual({ host: "fe80::1%eth0", port: 2222, user: "me" });
    expect(parseConnectionDetails("OCTOBOARD+SSH://me@box/")).toEqual({ host: "box", port: 22, user: "me" });
    expect(parseConnectionDetails("octoboard+ssh://me@[::1]")).toEqual({ host: "::1", port: 22, user: "me" });
  });

  it("skips fields it does not know and empty ones", () => {
    expect(parseConnectionDetails("octoboard+ssh://me@box:22?later=1&fingerprint=&flag")).toEqual({ host: "box", port: 22, user: "me" });
  });

  it("refuses text that is not connection details", () => {
    for (const text of [
      "",
      "ssh://me@box:22",
      "octoboard+ssh://box:22",
      "octoboard+ssh://@box:22",
      "octoboard+ssh://me@:22",
      "octoboard+ssh://me@box:0",
      "octoboard+ssh://me@box:65536",
      "octoboard+ssh://me@box:ssh",
      "octoboard+ssh://me@[::1",
      "octoboard+ssh://me@[::1]x",
      "octoboard+ssh://a%20b@box",
      "octoboard+ssh://me@box?name=%E0%A4",
    ]) {
      expect(parseConnectionDetails(text), text).toBeUndefined();
    }
  });
});
