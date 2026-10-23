import { Popover } from "@heroui/react";
import React from "react";

import { useT } from "../i18n/react";
import { AlertButton } from "./AlertButton";
import { Button } from "./Button";
import { CodeToneContext } from "./codeTone";

/** The Details button and the popover it opens: a dialog the button names, that Escape closes and gives
 * focus back from. It holds the further explanation in the application's own words, then the raw text
 * that was not written for the reader; the sentence saying what happened and the main remedy are never
 * behind it. Its text keeps the muted small style of the alert it belongs to. `placement` is where
 * the popover opens from the button: the end edge under an alert's buttons, the middle under a
 * centred row. */
export function DetailsButton({
  placement = "bottom end",
  inAlert = false,
  children,
}: {
  placement?: "bottom" | "bottom end";
  /** For the Details of a compact Alert, drawn in the Alert's status colour (`AlertButton`); the
   * empty-state panel's keeps the usual button styles. */
  inAlert?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  const label = t("error.details");
  return (
    <Popover>
      {inAlert ? <AlertButton kind="details" size="sm">{label}</AlertButton> : <Button size="sm" variant="secondary">{label}</Button>}
      <Popover.Content placement={placement} className="control-fills max-w-96">
        <Popover.Dialog aria-label={label} className="flex flex-col gap-2 p-3 text-sm text-app-muted [overflow-wrap:anywhere]">
          {/* The popover is not on the Alert's fill, so its chips leave the Alert's tone and take their own. */}
          <CodeToneContext.Provider value={undefined}>{children}</CodeToneContext.Provider>
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  );
}
