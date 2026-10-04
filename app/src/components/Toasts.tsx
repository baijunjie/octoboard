import React from "react";

import { sessionLocation } from "../sessionLabel";
import { useDaemon } from "../store";

/**
 * Surfaces every undismissed toast in one stack, in arrival order — including the per-volume
 * file-access failures of `list_dir` and `open_session`, which are a normal path here rather than
 * something to swallow (see "Known pitfalls of the Tauri / Rust approach" in docs/mvp.md). A
 * `notice` renders as its own visual variant, since it is something the user has to know rather
 * than something that went wrong, and names the session it is about — the daemon's message
 * deliberately does not, since it has no notion of what the client calls that session.
 */
export function Toasts(): React.ReactElement {
  const { toasts, sessions, consoles, projects, dismissToast } = useDaemon();
  if (toasts.length === 0) return <></>;
  return (
    <div className="toasts">
      {toasts.map((toast) => {
        const session = toast.session ? sessions.get(toast.session) : undefined;
        return (
          <div key={toast.id} className={`toast${toast.kind === "notice" ? " toast-notice" : ""}`}>
            <span>
              {session && <strong>{sessionLocation(session, consoles, projects)}: </strong>}
              {toast.message}
            </span>
            <button type="button" onClick={() => dismissToast(toast.id)} aria-label="Dismiss">
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}
