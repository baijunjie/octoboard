import React, { useEffect, useState } from "react";

import type { DirEntry, Event } from "../protocol";
import { useDaemon } from "../store";
import { Modal } from "./Modal";

interface DirectoryPickerProps {
  title: string;
  /** Starting path; `~` resolves to the host's home directory. */
  initialPath?: string;
  onPick: (path: string) => void;
  onClose: () => void;
}

/**
 * Browses directories through the daemon's `list_dir`, never the local filesystem directly — this
 * is the host role's job (`daemon/PROTOCOL.md`), and a remote host has no local file dialog to fall
 * back to. Only directories are ever listed, each flagged with whether it is a git repository.
 *
 * Failure here is a normal path, not an edge case: a packaged application raises a macOS
 * file-access prompt per volume, and the user may decline it or leave it unanswered (see "Known
 * pitfalls of the Tauri / Rust approach" in docs/mvp.md), so a failed listing is shown inline,
 * alongside whatever was listed before it, rather than clearing the screen.
 */
export function DirectoryPicker({ title, initialPath, onPick, onClose }: DirectoryPickerProps): React.ReactElement {
  const { request } = useDaemon();
  const [path, setPath] = useState(initialPath ?? "~");
  const [entries, setEntries] = useState<DirEntry[]>();
  const [resolvedPath, setResolvedPath] = useState<string>();
  const [error, setError] = useState<string>();

  const load = async (targetPath: string) => {
    setError(undefined);
    try {
      const event = (await request({ type: "list_dir", path: targetPath })) as Extract<Event, { type: "dir_listing" }>;
      setResolvedPath(event.path);
      setEntries(event.entries);
      // Keeps the field in sync with what was actually resolved (e.g. `~` becoming the real home
      // directory), which is what lets "Select this directory" below simply compare the two rather
      // than needing its own notion of "has the user edited this since the last listing".
      setPath(event.path);
    } catch (err) {
      setError((err as Error).message);
      // Deliberately not clearing `entries`/`resolvedPath`: a denied or not-yet-answered volume
      // prompt is the expected failure here, and it should not yank the previous listing (and its
      // `..` row back up) out from under the user along with it.
    }
  };

  useEffect(() => {
    void load(path);
    // Only on mount — subsequent navigation calls `load` directly so the input field can be edited
    // freely without triggering a listing on every keystroke. `path`'s initial value is read once
    // via `useState`'s initializer above, so it is intentionally not a dependency here.
  }, []);

  const parentOf = (p: string): string => {
    const segments = p.split("/").filter(Boolean);
    segments.pop();
    return "/" + segments.join("/");
  };

  // The path field can be edited without re-listing (see above), so it can name a directory that
  // was never actually resolved — picking it would hand back something the user never saw listed.
  const canSelect = !!resolvedPath && path === resolvedPath;

  const footer = (
    <>
      <button type="button" onClick={onClose}>
        Cancel
      </button>
      <button type="button" disabled={!canSelect} onClick={() => resolvedPath && onPick(resolvedPath)}>
        Select this directory
      </button>
    </>
  );

  return (
    <Modal title={title} onClose={onClose} footer={footer} onSubmit={() => load(path)}>
      <div className="directory-picker">
        <div className="directory-picker-path-row">
          <input type="text" value={path} onChange={(e) => setPath(e.target.value)} />
          <button type="submit">Go</button>
        </div>
        {error && <p className="error-text">{error}</p>}
        {entries && (
          <ul className="directory-picker-list">
            {resolvedPath && resolvedPath !== "/" && (
              <li>
                <button type="button" onClick={() => { const p = parentOf(resolvedPath); void load(p); }}>
                  ..
                </button>
              </li>
            )}
            {entries.map((entry) => (
              <li key={entry.path}>
                <button type="button" className="directory-picker-entry" onClick={() => void load(entry.path)}>
                  {entry.name}
                  {entry.is_git_repo && <span className="git-badge">git</span>}
                </button>
              </li>
            ))}
            {entries.length === 0 && <li className="directory-picker-empty">No subdirectories.</li>}
          </ul>
        )}
      </div>
    </Modal>
  );
}
