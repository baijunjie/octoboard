import { useEffect } from "react";

import { ActionableError } from "../errorRecovery";
import { useDaemon } from "../store";

/** A dialog that cannot be used as it stands (what it would choose from has gone while it was open) closes at
 * once and says why in a plain alert with nothing to act on. `message` is that alert's text. */
export function useCloseWhenBlocked(blocked: boolean, message: string, onClose: () => void): void {
  const { toastError } = useDaemon();
  useEffect(() => {
    if (!blocked) return;
    onClose();
    toastError(new ActionableError(message, "none"));
  }, [blocked]);
}
