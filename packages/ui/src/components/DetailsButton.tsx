import { Popover } from "@heroui/react";
import React from "react";

import { useT } from "../i18n/react";
import { AlertButton } from "./AlertButton";
import { CodeToneContext } from "./codeTone";

/** The Details button of a compact Alert and the popover it opens: a dialog the button names, that Escape
 * closes and gives focus back from. It holds the further explanation in the application's own words, then
 * the raw text that was not written for the reader; the sentence saying what happened and the main remedy
 * are never behind it. The button takes the Alert's status colour (`AlertButton`), and the popover's text
 * keeps the muted small style of the Alert it belongs to. */
export function DetailsButton({ children }: { children: React.ReactNode }): React.ReactElement {
  const t = useT();
  const label = t("error.details");
  return (
    <Popover>
      <AlertButton kind="details" size="sm">{label}</AlertButton>
      <Popover.Content placement="bottom end" className="control-fills max-w-96">
        <Popover.Dialog aria-label={label} className="flex flex-col gap-2 p-3 text-sm text-app-muted [overflow-wrap:anywhere]">
          {/* The popover is not on the Alert's fill, so its chips leave the Alert's tone and take their own. */}
          <CodeToneContext.Provider value={undefined}>{children}</CodeToneContext.Provider>
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  );
}
