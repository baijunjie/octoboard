// @vitest-environment jsdom
import { Terminal } from "@xterm/xterm";
import { describe, expect, it } from "vitest";

/** The real xterm, to pin what `TerminalController` relies on through private API: if an upgrade
 * renames `_core.coreService.onUserInput` or changes what it marks, this fails. */
type Core = { coreService: { onUserInput: (listener: () => void) => unknown } };

function observe(): { term: Terminal; events: string[] } {
  const term = new Terminal();
  const events: string[] = [];
  (term as unknown as { _core: Core })._core.coreService.onUserInput(() => events.push("user"));
  term.onData((data) => events.push(data));
  return { term, events };
}

describe("xterm's user-input mark", () => {
  it("comes right before the data of input, and not before data with `wasUserInput` false", () => {
    const { term, events } = observe();
    term.input("x");
    term.input("y", false);
    expect(events).toEqual(["user", "x", "y"]);
  });

  it("is absent from its own answer to a query in the output", async () => {
    const { term, events } = observe();
    await new Promise<void>((resolve) => term.write("\x1b[c", resolve));
    expect(events).toHaveLength(1);
    expect(events[0]).toMatch(/^\x1b\[\?/);
  });
});
