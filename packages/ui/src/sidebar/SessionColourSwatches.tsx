import React from "react";

import { type ConsoleSessionColour } from "../protocol";
import { sessionColourHex, CONSOLE_SESSION_COLOURS } from "../sessions/sessionColours";
import { useOctoboardTheme } from "../preferences/theme";
import { ColorSwatch, ColorSwatchPicker } from "../primitives";

/** How many swatches a row holds; `grid-cols-8` below. */
const COLUMNS = 8;

/** The palette as a grid of swatches (the order of `CONSOLE_SESSION_COLOURS`), the content of the
 * action menu's Colour submenu. Choosing a swatch applies it at once and closes the menu; the arrow
 * keys only move focus, Enter or Space on the focused swatch chooses it.
 *
 * Each swatch holds its real colour for the appearance in force, so react-aria names it in the
 * user's language, types ahead on that name, and rings the selected one in its own colour; no name
 * is written out here. Chosen again, the current colour just closes the menu. */
export function SessionColourSwatches({
  colour,
  onChoose,
  onClose,
  labelledBy,
  autoFocus,
}: {
  /** The session's colour now. */
  colour: ConsoleSessionColour | null | undefined;
  onChoose: (colour: ConsoleSessionColour) => void;
  onClose: () => void;
  /** The submenu's item, which names the list of swatches. */
  labelledBy: string | undefined;
  /** Whether focus goes into the grid, onto the current swatch, as it opens (a keyboard open; a
   * hover leaves it on the menu's item). */
  autoFocus: boolean;
}): React.ReactElement {
  const { resolved } = useOctoboardTheme();
  const swatches = CONSOLE_SESSION_COLOURS.flatMap((entry) => {
    const value = sessionColourHex(entry, resolved);
    return value ? [{ entry, value }] : [];
  });
  const current = swatches.find((swatch) => swatch.entry === colour)?.value;

  // Pressing the current swatch changes no selection, so it closes the menu from here (in the
  // capture phase: a press stops the event on its swatch); pressing another one is closed by its
  // selection.
  const closeOnCurrent = (event: React.SyntheticEvent) => {
    if ((event.target as Element).closest('[role="option"]')?.getAttribute("aria-selected") === "true") onClose();
  };

  // A virtual click (a screen reader's activation) focuses the list box itself rather than a
  // swatch, and so does a current colour outside the palette; focus is handed on to the selected
  // swatch, or the first one, so the list box is never what holds it.
  const focusSwatch = (event: React.FocusEvent<HTMLElement>) => {
    if (event.target.getAttribute("role") !== "listbox") return;
    const options = event.target.querySelectorAll<HTMLElement>('[role="option"]');
    (Array.from(options).find((option) => option.getAttribute("aria-selected") === "true") ?? options[0])?.focus();
  };

  return (
    <div
      onFocus={focusSwatch}
      onClickCapture={closeOnCurrent}
      onKeyDownCapture={(event) => (event.key === "Enter" || event.key === " ") && closeOnCurrent(event)}
    >
      {/* `autoFocus` puts focus into the picker, on the current swatch, where the arrow keys go on from, rather than
          on the dialog around it. */}
      <ColorSwatchPicker
        labelledBy={labelledBy}
        autoFocus={autoFocus}
        className="grid grid-cols-8 gap-1.5 p-2"
        value={current}
        onChange={(picked) => {
          const chosen = swatches.find((swatch) => swatch.value === picked);
          if (chosen) onChoose(chosen.entry);
        }}
      >
        {swatches.map(({ entry, value }, index) => (
          <ColorSwatch key={entry} color={value} swatchClassName="session-colour-ring" data-panel-edge={index % COLUMNS === 0 ? "start" : undefined} />
        ))}
      </ColorSwatchPicker>
    </div>
  );
}
