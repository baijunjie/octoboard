// @vitest-environment jsdom
import { act } from "react";
import { FocusScope } from "react-aria";
import { expect, it } from "vitest";

import { FOCUS_SCOPE_RESTORE_EVENT } from "./focus";
import { Table, TableBody, TableCell, TableColumn, TableHeader, TableRow } from "./Table";
import { mount } from "./testSupport";

function controls(after = true) {
  const { container } = mount(
    <FocusScope contain>
      <button>Before</button>
      <h2 id="servers">Servers</h2>
      <Table labelledBy="servers" minWidth="34rem">
        <TableHeader>
          <TableColumn rowHeader>Name</TableColumn>
          <TableColumn>Runtime</TableColumn>
          <TableColumn>Actions</TableColumn>
        </TableHeader>
        <TableBody>
          <TableRow id="first" textValue="First">
            <TableCell>First</TableCell>
            <TableCell>Running</TableCell>
            <TableCell><button>Sign in</button></TableCell>
          </TableRow>
          <TableRow id="second" textValue="Second">
            <TableCell>Second</TableCell>
            <TableCell><button aria-label="Error details" /></TableCell>
            <TableCell>
              <button>Cancel</button>
              <button disabled>Busy</button>
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
      {after && <button>After</button>}
    </FocusScope>,
  );
  const button = (name: string) => [...container.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => (button.getAttribute("aria-label") ?? button.textContent) === name)!;
  return { container, button };
}

// Focus lands and React commits its effects before the key is pressed, as it does for a person: the table notes the
// cell that took focus when the modality is not the keyboard's, and the effect that follows must find focus where it is.
function tab(from: HTMLElement, init: KeyboardEventInit = {}): void {
  act(() => from.focus());
  act(() => {
    from.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true, ...init }));
  });
}

it.each([false, true])("Tab visits controls across cells and rows and then leaves the table, with Option %s", (altKey) => {
  const { button } = controls();
  for (const [from, to] of [["Sign in", "Error details"], ["Error details", "Cancel"], ["Cancel", "After"]]) {
    tab(button(from), { altKey });
    expect(document.activeElement).toBe(button(to));
  }
});

it.each([false, true])("Shift+Tab visits the preceding controls across cells and rows, with Option %s", (altKey) => {
  const { button } = controls();
  for (const [from, to] of [["Cancel", "Error details"], ["Error details", "Sign in"]]) {
    tab(button(from), { altKey, shiftKey: true });
    expect(document.activeElement).toBe(button(to));
  }
});

it("keeps Tab inside the enclosing focus scope when the table holds its last control", () => {
  const { container, button } = controls(false);
  tab(button("Cancel"));
  expect(document.activeElement).toBe(button("Before"));
  expect(container.contains(document.activeElement)).toBe(true);
});

it("keeps reverse entry on the last control after focus has left the table", () => {
  const { button } = controls();
  act(() => {
    button("Sign in").focus();
    button("After").focus();
    // jsdom has no native Tab default action. Focus the element reverse Tab reaches, so the
    // real collection's focus-entry handling runs with the previously visited row remembered.
    button("Cancel").focus();
  });
  expect(document.activeElement).toBe(button("Cancel"));
});

it("lets a closing popover restore its original table control", () => {
  const { button } = controls();
  const restore = new Event(FOCUS_SCOPE_RESTORE_EVENT, { bubbles: true, cancelable: true });
  act(() => {
    button("Error details").focus();
    button("Error details").dispatchEvent(restore);
  });
  // A cancelled restore tells FocusScope to abandon its original control. The real collection's
  // listener is mounted here, so this also detects it taking over restoration again.
  expect(restore.defaultPrevented).toBe(false);
});
