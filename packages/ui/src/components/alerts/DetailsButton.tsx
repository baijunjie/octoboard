import React from "react";

import { useT } from "../../i18n/react";
import { usePopoverOwnerProps } from "../hint/popoverOwner";
import { AlertButton } from "./AlertButton";
import { CodeToneContext } from "../../i18n/codeTone";
import { Popover, PopoverContent } from "../../primitives";

/** Marks the popover of a Details button, so that a panel it is shown from can tell it is part of that panel: it is
 * portalled out of it (`PopoverOwnerContext` names the panel). */
export const DETAILS_POPOVER = "[data-details-popover]";

/** The Details button of a compact Alert and the popover it opens: a dialog the button names, that Escape
 * closes and gives focus back from. It holds the further explanation in the application's own words, then
 * the raw text that was not written for the reader; the sentence saying what happened and the main remedy
 * are never behind it. The button takes the Alert's status colour (`AlertButton`), and the popover's text
 * keeps the muted small style of the Alert it belongs to. */
export function DetailsButton({ children }: { children: React.ReactNode }): React.ReactElement {
  const t = useT();
  const label = t("error.details");
  const owner = usePopoverOwnerProps();
  return (
    <Popover>
      <AlertButton kind="details" size="sm">{label}</AlertButton>
      <PopoverContent
        placement="bottom end"
        label={label}
        className="control-fills max-w-96"
        dialogClassName="flex flex-col gap-2 p-3 text-sm text-app-muted [overflow-wrap:anywhere]"
        data-details-popover=""
        {...owner}
      >
        {/* The popover is not on the Alert's fill, so its chips leave the Alert's tone and take their own. */}
        <CodeToneContext.Provider value={undefined}>{children}</CodeToneContext.Provider>
      </PopoverContent>
    </Popover>
  );
}
