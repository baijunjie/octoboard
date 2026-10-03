import React from "react";

import { useDaemon } from "../store";

/** Surfaces every daemon-reported `error` the user has not dismissed yet — including the
 * per-volume file-access failures of `list_dir` and `open_session`, which are a normal path here
 * rather than something to swallow (see "Known pitfalls of the Tauri / Rust approach" in
 * docs/mvp.md). */
export function ErrorToasts(): React.ReactElement {
  const { errors, dismissError } = useDaemon();
  if (errors.length === 0) return <></>;
  return (
    <div className="error-toasts">
      {errors.map((error) => (
        <div key={error.id} className="error-toast">
          <span>{error.message}</span>
          <button type="button" onClick={() => dismissError(error.id)} aria-label="Dismiss">
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
