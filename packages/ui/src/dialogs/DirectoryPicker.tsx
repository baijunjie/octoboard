import { Button, Chip, Input, ListBox } from "@heroui/react";
import React, { useEffect, useRef, useState } from "react";

import { DaemonRequestError } from "../daemon-client";
import { useT } from "../i18n/react";
import type { DirEntry, Event } from "../protocol";
import { useDaemon } from "../store";
import { Dialog, DialogError, useRefocusIfLost } from "./Dialog";

/** The ids of the list's two entries that are not a directory. A real entry's id is its absolute
 * path, which neither of these can be. */
const PARENT_KEY = "..";
const EMPTY_KEY = "empty";

/** The directory containing `path`, `~` once there is nothing above it. */
function ancestorOf(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const slash = trimmed.lastIndexOf("/");
  if (slash < 0) return "~";
  return slash === 0 ? "/" : trimmed.slice(0, slash);
}

/**
 * Browses directories through the daemon's `list_dir`, never the local filesystem directly — this
 * is the host role's job (`apps/daemon/PROTOCOL.md`), and a remote host has no local file dialog to fall
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
  const t = useT();
  const { request } = useDaemon();
  const [path, setPath] = useState(initialPath ?? "~");
  const [entries, setEntries] = useState<DirEntry[]>();
  const [resolvedPath, setResolvedPath] = useState<string>();
  const [error, setError] = useState<string>();
  const listRef = useRef<HTMLDivElement>(null);

  const load = async (targetPath: string, fallBackToAncestor = false) => {
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
      // A starting path that does not exist yet (a default clone directory not yet created) would
      // leave nothing to navigate from, so the first listing walks up to the nearest ancestor that
      // lists. Only that failure does: a declined volume prompt or an unreadable directory is
      // shown as it is.
      if (fallBackToAncestor && targetPath !== "~" && err instanceof DaemonRequestError && err.code === "path_not_found") {
        return load(ancestorOf(targetPath), true);
      }
      setError((err as Error).message);
      // Deliberately not clearing `entries`/`resolvedPath`: a denied or not-yet-answered volume
      // prompt is the expected failure here, and it should not yank the previous listing (and its
      // `..` row back up) out from under the user along with it.
    }
  };

  useEffect(() => {
    void load(path, true);
    // Only on mount — subsequent navigation calls `load` directly so the input field can be edited
    // freely without triggering a listing on every keystroke. `path`'s initial value is read once
    // via `useState`'s initializer above, so it is intentionally not a dependency here.
  }, []);

  // Loading replaces the list, unmounting the entry that was pressed. The list itself is the
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
        {t("common.cancel")}
      </Button>
      <Button type="button" isDisabled={!canSelect} onPress={() => resolvedPath && onPick(resolvedPath)}>
        {t("directoryPicker.select")}
      </Button>
    </>
  );

  return (
    <Dialog title={title} onClose={onClose} footer={footer} onSubmit={() => void load(path)}>
      <div className="flex gap-2">
        <Input fullWidth variant="secondary" dir="ltr" aria-label={t("directoryPicker.path")} value={path} onChange={(e) => setPath(e.target.value)} />
        <Button type="submit" variant="secondary">
          {t("directoryPicker.go")}
        </Button>
      </div>
      <DialogError message={error} />
      {entries && (
        <ListBox
          ref={listRef}
          aria-label={t("directoryPicker.subdirectories")}
          // `selectionMode` stays `none`: an entry is only ever entered, never picked. Activating
          // one (click, Enter) calls `onAction`. The padding keeps the scroll container from
          // clipping the items' focus ring.
          onAction={(key) => void load(key === PARENT_KEY ? parentOf(resolvedPath ?? "/") : String(key))}
          className="max-h-72 overflow-y-auto rounded-lg border border-separator p-1"
        >
          {resolvedPath && resolvedPath !== "/" && (
            <ListBox.Item id={PARENT_KEY} textValue={t("directoryPicker.parent")} aria-label={t("directoryPicker.parent")}>
              ..
              <span className="ms-2 text-muted">{t("directoryPicker.parent")}</span>
            </ListBox.Item>
          )}
          {entries.map((entry) => (
            <ListBox.Item key={entry.path} id={entry.path} textValue={entry.name}>
              <span dir="auto">{entry.name}</span>
              {entry.is_git_repo && (
                <Chip size="sm" variant="soft" className="ms-2">
                  git
                </Chip>
              )}
            </ListBox.Item>
          ))}
          {entries.length === 0 && (
            // Inside the list, as an option that cannot be acted on: arrow keys skip it, and the
            // list is never left without an option, which a listbox needs.
            <ListBox.Item id={EMPTY_KEY} isDisabled textValue={t("directoryPicker.empty")}>
              {t("directoryPicker.empty")}
            </ListBox.Item>
          )}
        </ListBox>
      )}
    </Dialog>
  );
}
