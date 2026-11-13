import { RotateCcw } from "lucide-react";
import React, { useContext, useRef } from "react";

import { TitledControl } from "../components/hint/TitledControl";
import { useT } from "../i18n/react";
import { interactionIsTouch } from "../input/lastInput";
import { Button, NumberField } from "../primitives";
import { DEFAULT_TERMINAL_FONT_SIZE, MAX_TERMINAL_FONT_SIZE, MIN_TERMINAL_FONT_SIZE, resetTerminalFontSize, terminalFontSize } from "../terminal/terminalFontSize";
import { SettingRow } from "./SettingRow";
import { SettingsFocusContext } from "./useSectionRefocus";

/** The font size of the terminals, in whole pixels: a number field with stepper buttons, named by the row's label,
 * with a button back to the default size before it, shown only while the size is not the default. The button keeps
 * its place when hidden, so the field does not move as it comes and goes; pressing it hides it, so focus moves on
 * rather than being lost: to the field, or after a tap to the selected section tab, as a move into a text field
 * raises the on-screen keyboard. A size typed in is taken on blur or Enter, held to the range, and every change
 * applies to the terminals at once. */
export function TerminalFontSizeSetting(): React.ReactElement {
  const t = useT();
  const size = terminalFontSize.useValue();
  const input = useRef<HTMLInputElement>(null);
  const sectionTab = useContext(SettingsFocusContext);
  const reset = t("settings.terminalFontSize.reset");
  const isDefault = size === DEFAULT_TERMINAL_FONT_SIZE;
  return (
    <SettingRow fill searchId="terminal-font-size" label={t("settings.terminalFontSize.label")} description={t("settings.terminalFontSize.description")}>
      <div className="flex min-w-0 items-center gap-2">
        <div className={isDefault ? "invisible" : undefined} aria-hidden={isDefault || undefined}>
          <TitledControl title={reset}>
            <Button
              size="sm"
              variant="tertiary"
              iconOnly
              label={reset}
              disabled={isDefault}
              onPress={() => {
                resetTerminalFontSize();
                (interactionIsTouch() ? sectionTab() : input.current)?.focus();
              }}
            >
              <RotateCcw aria-hidden="true" className="size-4" />
            </Button>
          </TitledControl>
        </div>
        <NumberField
          label={t("settings.terminalFontSize.label")}
          value={size}
          onChange={(next) => terminalFontSize.set(next)}
          min={MIN_TERMINAL_FONT_SIZE}
          max={MAX_TERMINAL_FONT_SIZE}
          incrementLabel={t("settings.shortcuts.command.increaseTerminalFontSize")}
          decrementLabel={t("settings.shortcuts.command.decreaseTerminalFontSize")}
          inputRef={input}
          variant="secondary"
          className="w-48 max-tablet:flex-1"
        />
      </div>
    </SettingRow>
  );
}
