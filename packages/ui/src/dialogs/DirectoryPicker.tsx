import { Button, Input } from "@heroui/react";
import React, { useEffect, useRef, useState } from "react";

import type { DirEntry, Event } from "../protocol";
import { useDaemon } from "../store";
import { Dialog, DialogError, useRefocusIfLost } from "./Dialog";

/**
 * Browses directories through the daemon's `list_dir`, never the local filesystem directly — this
 * is the host role's job (`daemon/PROTOCOL.md`), and a remote host has no local file dialog to fall
 * back to. Only directories are ever listed, each flagged with whether it is a git repository.
 *
 * Failure here is a normal path, not an edge case: a packaged application raises a macOS
 * file-access prompt per volume, and the user may decline it or leave it unanswered (see "Known
 * pitfalls of the Tauri / Rust approach" in docs/architecture.md), so a failed listing is shown
 * inline, alongside whatever was listed before it, rather than clearing the screen.
 */
export function DirectoryPicker({
  title,
  initialPath,
  onPick,
  onClose,
}: {
  title: string;
  /** Starting path; `~` resolves to the host's home directory. */
  initialPath?: string;
  onPick: (path: string) => void;
  onClose: () => void;
}): React.ReactElement {
  const { request } = useDaemon();
  const [path, setPath] = useState(initialPath ?? "~");
  const [entries, setEntries] = useState<DirEntry[]>();
  const [resolvedPath, setResolvedPath] = useState<string>();
  const [error, setError] = useState<string>();
  const listRef = useRef<HTMLUListElement>(null);

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

  // Loading replaces the list, unmounting the entry button that was pressed. The list itself is the
  // stable place to put focus back, so Tab carries on from the entries.
  useRefocusIfLost(() => listRef.current, [entries]);

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
      <Button type="button" variant="secondary" onPress={onClose}>
        Cancel
      </Button>
      <Button type="button" isDisabled={!canSelect} onPress={() => resolvedPath && onPick(resolvedPath)}>
        Select this directory
      </Button>
    </>
  );

  return (
    <Dialog title={title} onClose={onClose} footer={footer} onSubmit={() => void load(path)}>
      <div className="flex gap-2">
        <Input fullWidth aria-label="Directory path" value={path} onChange={(e) => setPath(e.target.value)} />
        <Button type="submit" variant="secondary">
          Go
        </Button>
      </div>
      <DialogError message={error} />
      {entries && (
        <ul
          ref={listRef}
          tabIndex={-1}
          className="max-h-72 overflow-y-auto rounded-lg border border-separator outline-none focus-visible:ring-2 focus-visible:ring-focus"
        >
          {resolvedPath && resolvedPath !== "/" && (
            <li>
              <Button fullWidth variant="ghost" className="justify-start" onPress={() => void load(parentOf(resolvedPath))}>
                ..
              </Button>
            </li>
          )}
          {entries.map((entry) => (
            <li key={entry.path}>
              <Button fullWidth variant="ghost" className="justify-start" onPress={() => void load(entry.path)}>
                {entry.name}
                {entry.is_git_repo && <span className="ml-2 rounded bg-default px-1 text-xs text-muted">git</span>}
              </Button>
            </li>
          ))}
          {entries.length === 0 && <li className="px-3 py-2 text-sm text-muted">No subdirectories.</li>}
        </ul>
      )}
    </Dialog>
  );
}
