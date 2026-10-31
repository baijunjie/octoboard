// Shared by the tests of what waits for the user to be in the window (`useWindowAttended`).
import { act } from "react";
import { onTestFinished, vi } from "vitest";

/** Puts the window in or out of the user's attention for one test, through its focus: jsdom's `hasFocus()` answers
 * from whether anything in the document was ever focused, not from the window. Starts attended. */
export function windowAttention(): (attended: boolean) => void {
  let focused = true;
  const spy = vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
  onTestFinished(() => spy.mockRestore());
  return (attended) => act(() => {
    focused = attended;
    window.dispatchEvent(new Event(attended ? "focus" : "blur"));
  });
}
