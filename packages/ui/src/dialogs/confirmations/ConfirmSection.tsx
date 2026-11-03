import React, { useId } from "react";

import { KeepFocusInDialog } from "../../components/dialog/KeepFocusInDialog";

/** One topic of a sectioned confirmation (the archive's, the project removal's): a labelled group whose `<h3>` heading names it (the
 * dialog's own title is the `<h2>`), under a groove divider when `divided`, i.e. when another section is
 * shown above it. `shown` false renders nothing but keeps the wrapper mounted, so focus that was
 * inside when the section went away goes back to the dialog. */
export function ConfirmSection({ heading, shown, divided, children }: {
  heading: string;
  shown: boolean;
  divided: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  const headingId = useId();
  return (
    <KeepFocusInDialog>
      {shown && (
        <div role="group" aria-labelledby={headingId} className={`flex min-w-0 flex-col gap-2 text-sm ${divided ? "groove-t pt-3" : ""}`}>
          <h3 id={headingId} className="font-semibold">{heading}</h3>
          {children}
        </div>
      )}
    </KeepFocusInDialog>
  );
}
