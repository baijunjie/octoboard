// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";

import { refocusIfLost } from "./refocusIfLost";

describe("refocusIfLost", () => {
  afterEach(() => document.body.replaceChildren());

  it("hands focus to the terminal when it is on the body or on a disabled control", () => {
    const focusTerminal = vi.fn();
    refocusIfLost(focusTerminal);
    expect(focusTerminal).toHaveBeenCalledTimes(1);

    const button = document.body.appendChild(document.createElement("button"));
    button.focus();
    refocusIfLost(focusTerminal);
    expect(focusTerminal).toHaveBeenCalledTimes(1);

    button.disabled = true;
    refocusIfLost(focusTerminal);
    expect(focusTerminal).toHaveBeenCalledTimes(2);
  });
});
