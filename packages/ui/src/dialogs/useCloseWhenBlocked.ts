import { useEffect } from "react";

import { ActionableError } from "../errorRecovery";
import { useDaemon } from "../store";

/** A dialog that cannot be used as it stands (what it would choose from has gone while it was open) closes at
 * once and says why in an acknowledgement-only alert. Structured errors retain the metadata needed to
 * render names in the explanation. */
export function useCloseWhenBlocked(blocked: boolean, message: string | ActionableError, onClose: () => void): void {
  const { toastError } = useDaemon();
  useEffect(() => {
    if (!blocked) return;
    onClose();
    toastError(typeof message === "string" ? new ActionableError(message, "none") : message);
  }, [blocked]);
}
