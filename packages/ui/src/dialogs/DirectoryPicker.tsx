import { Description, Input, ListBox } from "@heroui/react";
import { CornerLeftUp, Folder, FolderX } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { Button } from "../components/Button";
import { EmptyPanel } from "../components/EmptyPanel";
import { FailureAlert } from "../components/FailureAlert";
import { PathText } from "../components/MarkedPath";
import { useStickyHeadingFocus } from "../components/stickyHeadingFocus";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { StatusBadge } from "../components/StatusBadge";
import { DaemonRequestError } from "../daemon-client";
import { FadeOverflow } from "../components/FadeOverflow";
import { useStickyScrollFade } from "../components/scrollFade";
import { errorPresentation, type ErrorPresentation } from "../errorPresentation";
import { joinPhrases } from "../i18n/joinPhrases";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { DirEntry, Event } from "../protocol";
import { abbreviateHome } from "../pathDisplay";
import { useDaemon, useDaemonStore } from "../store";
import { Dialog, useRefocusIfLost } from "./Dialog";
import { useTrimmedField } from "./useTrimmedField";

/** The ids of the list's two entries that are not a directory. A real entry's id is its absolute
 * path, which neither of these can be. */
const PARENT_KEY = "..";
const EMPTY_KEY = "empty";

/** The parent-directory row's height, the 4px the list keeps above it for its focus ring included:
 * what the row occupies while it is pinned, and so where the fade starts. */
const PARENT_ROW_HEIGHT = 40;

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
 * pitfalls of the Tauri / Rust approach" in docs/architecture.md), so a failed listing is the state of
 * what the picker shows: an Alert under the path field with the reason and Try again, while the
 * previous listing stays available.
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
  const pathField = useTrimmedField(path, setPath);
  const [entries, setEntries] = useState<DirEntry[]>();
  const [resolvedPath, setResolvedPath] = useState<string>();
  // The last listing that failed and why, until a listing succeeds; `count` numbers the failures, so
  // one that repeats in the same words is announced again, and `initial` marks the listing made as
  // the picker mounted.
  const [failure, setFailure] = useState<{ target: string; error: ErrorPresentation; count: number; initial: boolean }>();
  // Whether Try again's listing is out: its button stays where it is, unavailable, rather than
  // taking a second press, and the failure stays shown until the listing answers.
  const [retrying, setRetrying] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const load = async (targetPath: string, initial = false) => {
    try {
      const event = (await request({ type: "list_dir", path: targetPath })) as Extract<Event, { type: "dir_listing" }>;
      setFailure(undefined);
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
      if (initial && targetPath !== "~" && err instanceof DaemonRequestError && err.code === "path_not_found") {
        return load(ancestorOf(targetPath), true);
      }
      setFailure((last) => ({ target: targetPath, error: errorPresentation(err), count: (last?.count ?? 0) + 1, initial }));
      // Deliberately not clearing `entries`/`resolvedPath`: a denied or not-yet-answered volume
      // prompt is the expected failure here, and it should not yank the previous listing (and its
      // parent-directory row back up) out from under the user along with it.
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
        <Input autoFocus fullWidth variant="secondary" dir="ltr" aria-label={t("directoryPicker.path")} placeholder={t("directoryPicker.pathExample")} {...pathField} />
        <Button type="submit" variant="secondary">
          {t("directoryPicker.go")}
        </Button>
      </div>
      {failure && (
        // A listing is asked for after the picker opened, so its failure always announces itself. The
        // one for the listing made on mount may come while the path field is still taking focus, so
        // it waits that out.
        <FailureAlert
          announce
          announceDelayMs={failure.initial ? AFTER_FOCUS_MOVE_DELAY_MS : undefined}
          attempt={failure.count}
          title={t("directoryPicker.failed")}
          message={failure.error}
          pending={retrying}
          onRetry={() => {
            setRetrying(true);
            void load(failure.target).finally(() => setRetrying(false));
          }}
        />
      )}
      {entries && (
        <DirectoryList
          listRef={listRef}
          entries={entries}
          showParent={!!resolvedPath && resolvedPath !== "/"}
          onOpen={(key) => void load(key === PARENT_KEY ? parentOf(resolvedPath ?? "/") : key)}
        />
      )}
    </Dialog>
  );
}

/** The framed list of a listing's directories. It is a component of its own so the fade's hook
 * mounts together with the list: `useStickyScrollFade` binds to the scroller once, and the list
 * only exists after the first listing. */
function DirectoryList({ listRef, entries, showParent, onOpen }: {
  listRef: React.RefObject<HTMLDivElement | null>;
  entries: DirEntry[];
  /** Whether the listing has a directory above it (anything but `/`). */
  showParent: boolean;
  /** Receives the activated entry's id: `PARENT_KEY` or a directory's path. */
  onOpen: (key: string) => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const home = useDaemonStore((s) => s.homeDir);
  // The list scrolls on its own inside the dialog, which `Dialog`'s body fade does not reach. The
  // rows' focus ring and its offset fit within the `ListBox`'s own `p-1`, so the scroller does not
  // clip it. The parent-directory row stays pinned above the entries, so the fade is the overlay
  // kind that paints below it instead of a mask that would dim it.
  const { props: fade, wrap } = useStickyScrollFade({ ref: listRef, className: "max-h-72 overflow-y-auto", stickyHeadingHeight: showParent ? PARENT_ROW_HEIGHT : 0, stickySurface: "overlay" });
  const pinnedFocus = useStickyHeadingFocus(PARENT_ROW_HEIGHT);
  return (
    // The frame is this picker's own: it composes HeroUI's `ListBox` into a higher-level
    // browser and marks out the scrolling region within the dialog, an addition on top of
    // HeroUI rather than a redrawn default, so work that restores HeroUI's own look leaves it
    // alone. It sits outside the scroller so the scroll fade covers the rows, never the frame, and
    // is a flex column so the list shrinks with a short dialog body and its last rows stay
    // reachable rather than being cut off.
    <div className="flex min-h-0 flex-col overflow-hidden rounded-lg border border-separator">
      {wrap(
        <ListBox
          {...fade}
          aria-label={t("directoryPicker.subdirectories")}
          // `selectionMode` stays `none`: an entry is only ever entered, never picked. Activating
          // one (click, Enter) calls `onAction`.
          onAction={(key) => onOpen(String(key))}
        >
          {showParent && (
            // Pinned at the list's top, opaque so the entries scrolling under it do not show
            // through. A shadow paints the 4px the list keeps above the row in the same colour,
            // beneath the focus ring's own shadow layers, so the entries do not pass through that
            // strip either and the ring stays whole. Of the sticky-heading props only the scroll
            // hold of the focus handlers does anything here: the scroll margin and the heading
            // marker are for a heading's inner controls and for `FadedScroller`.
            <ListBox.Item
              {...pinnedFocus}
              data-marquee-scope
              id={PARENT_KEY}
              textValue={t("directoryPicker.parent")}
              aria-label={t("directoryPicker.parent")}
              className={`sticky top-0 z-3 bg-overlay shadow-[0_-4px_0_0_var(--color-overlay)] data-[hovered=true]:bg-default ${pinnedFocus.className}`}
            >
              <CornerLeftUp aria-hidden="true" className="size-4 shrink-0 text-app-muted rtl:-scale-x-100" />
              <FadeOverflow className="min-w-0 flex-1 text-app-muted" titleWhenClipped={t("directoryPicker.parent")}>{t("directoryPicker.parent")}</FadeOverflow>
            </ListBox.Item>
          )}
          {entries.map((entry) => (
            // The name and the path under it are two labels, so each runs its own marquee rather
            // than the row being one marquee scope. The row is named by the directory's name and
            // its git badge, leaving the path out of the name; the path is its description. That
            // label already joins the name and the badge, so the badge adds no comma of its own.
            <ListBox.Item key={entry.path} id={entry.path} textValue={entry.name} aria-label={entry.is_git_repo ? joinPhrases(language, [entry.name, "git"]) : entry.name}>
              {/* Centred on the name's 20px line rather than on the two-line row. */}
              <Folder aria-hidden="true" className="mt-0.5 size-4 shrink-0 self-start text-app-muted" />
              <span className="flex min-w-0 flex-1 flex-col">
                <FadeOverflow dir="auto" className="min-w-0 text-foreground" titleWhenClipped={entry.name}>
                  {entry.name}
                </FadeOverflow>
                <Description className="flex min-w-0">
                  {/* HeroUI turns pointer events off on an item's description, which would leave
                      the path without its tooltip and marquee. */}
                  <PathText path={entry.path} shown={abbreviateHome(entry.path, home)} className="pointer-events-auto min-w-0 flex-1" />
                </Description>
              </span>
              {entry.is_git_repo && <StatusBadge separated={false}>git</StatusBadge>}
            </ListBox.Item>
          ))}
          {entries.length === 0 && (
          // Inside the list, as an option that cannot be acted on: arrow keys skip it, and the
          // list is never left without an option, which a listbox needs. Its content is the shared
          // compact empty row, the form a nested list's nothing takes ("The compact row" in `docs/product/empty-states.md`),
          // so a message too long for the dialog wraps rather than being clipped; the option drops
          // its own padding and dimming, as the row carries the padding and its message is no
          // disabled control.
            <ListBox.Item id={EMPTY_KEY} isDisabled textValue={t("directoryPicker.empty")} className="p-0 data-[disabled=true]:opacity-100">
              <EmptyPanel compact icon={FolderX} message={t("directoryPicker.empty")} />
            </ListBox.Item>
          )}
        </ListBox>,
      )}
    </div>
  );
}
