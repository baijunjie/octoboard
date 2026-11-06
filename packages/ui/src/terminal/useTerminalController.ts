import { useEffect, useRef, type RefObject } from "react";

import { reportExternalLinkFailure } from "../components/text/ExternalLink";
import { useLatest } from "../components/hooks/useLatest";
import { useResizeDrag } from "../components/panel/resizeDrag";
import { useT } from "../i18n/react";
import { useOctoboardTheme } from "../preferences/theme";
import { usePlatform } from "../platform/react";
import type { ViewedFile } from "../viewer/viewedFile";
import { useDaemon } from "../store";
import { revealPathTarget } from "./links/revealPathTarget";
import { openPathTarget } from "./links/openPathTarget";
import type { HoveredTerminalLink } from "./links/terminalLinks";
import { programCopyHandler } from "./osc52";
import { TerminalController, type TerminalControllerOptions, type TermStatus } from "./TerminalController";

/**
 * The one `TerminalController` of a terminal view, built when the view mounts into `containerRef` and
 * disposed when it goes, with what keeps it in step with the view: links, clipboard and path links
 * (read from the latest render, as the controller is built once), a fit when the container resizes,
 * a single fit when `resizing` ends, the input's label in the UI language and the colour theme.
 *
 * `controllerRef` is the caller's, so that `useTerminalReconnect` can read the controller before
 * this hook has built it (that hook makes the `onStatusChange` this one needs). Attaching is the
 * caller's: the view decides when and to what.
 */
export function useTerminalController({
  controllerRef,
  containerRef,
  setLinkRef,
  resizing,
  shown = true,
  onStatusChange,
  onPaintedChange,
  project,
  viewFile,
  options,
}: {
  controllerRef: RefObject<TerminalController | undefined>;
  /** The element xterm mounts in. */
  containerRef: RefObject<HTMLElement | null>;
  /** Where the OSC 8 link hover goes: the tooltip holds the state itself, so hovering does not render the view. */
  setLinkRef: RefObject<((link: HoveredTerminalLink | undefined) => void) | undefined>;
  /** The panel is easing in size: the view keeps its size until the ease ends, then fits once. */
  resizing: boolean;
  /** Whether the view can be fitted. A hidden one has no size, and fits when it is shown. */
  shown?: boolean;
  /** Read once, when the controller is built: a later change of the callback is not followed. */
  onStatusChange: (status: TermStatus) => void;
  /** Read once, like `onStatusChange`. */
  onPaintedChange?: (painted: boolean) => void;
  /** The project the paths in the output are asked about, which the controller built once does not otherwise follow. */
  project: string | null | undefined;
  /** Shows a project file a path in the output named. Without it such a path opens in the default application. */
  viewFile?: (file: ViewedFile) => void;
  /** Read once, when the controller is built. */
  options?: TerminalControllerOptions;
}): void {
  const t = useT();
  const { request } = useDaemon();
  const { links, clipboard } = usePlatform();
  const { resolved: colorTheme } = useOctoboardTheme();
  const dragging = useResizeDrag();
  const holdingSize = resizing || dragging;
  const resizingRef = useLatest(holdingSize);
  const pendingFitRef = useRef(false);
  const latest = useLatest({ t, request, project: project ?? undefined, viewFile });
  // What the controller's theme currently is: the construction below reads it for the initial
  // value, and the `colorTheme` effect further down both reads and updates it. One ref rather than
  // two so the two can never claim different things about the same instance.
  const appliedColorThemeRef = useRef(colorTheme);

  useEffect(() => {
    const controller = new TerminalController(
      {
        onStatusChange,
        onPaintedChange,
        openLink: links && ((url) => links.open(url).catch((cause: unknown) => reportExternalLinkFailure(latest.current.t, url, cause))),
        onLinkHover: (link) => setLinkRef.current?.(link),
        copyToClipboard: programCopyHandler(clipboard, () => latest.current),
        pathLinks: {
          probe: async (paths) => {
            const reply = await latest.current.request({ type: "probe_path_links", project: latest.current.project, paths });
            return reply.links.map(({ kind, view, viewer_only }) => ({ kind, view, viewerOnly: viewer_only }));
          },
          open: (target, onHost) =>
            openPathTarget({
              target,
              onHost,
              project: latest.current.project,
              request: latest.current.request,
              viewFile: latest.current.viewFile,
            }),
          reveal: (target) => revealPathTarget({ target, project: latest.current.project, request: latest.current.request }),
        },
      },
      appliedColorThemeRef.current,
      options,
    );
    controllerRef.current = controller;
    if (containerRef.current) controller.mount(containerRef.current);
    const resizeObserver = new ResizeObserver(() => {
      // xterm clears its renderer on resize and repaints a frame later. Hold the grid through
      // continuous layout changes so intermediate sizes do not expose a cleared canvas.
      if (resizingRef.current) {
        pendingFitRef.current = true;
        return;
      }
      pendingFitRef.current = false;
      controller.fit();
      controller.syncSize();
    });
    if (containerRef.current) resizeObserver.observe(containerRef.current);
    return () => {
      resizeObserver.disconnect();
      controller.dispose();
    };
    // One controller for the life of the view; what changes while it lives comes through `latest`, bar the
    // callbacks and options documented as read once.
  }, []);

  // Only views whose containers changed need a catch-up fit; floating panes leave the agent alone.
  const wasResizingRef = useRef(false);
  useEffect(() => {
    const ended = wasResizingRef.current && !holdingSize;
    wasResizingRef.current = holdingSize;
    if (!ended || !shown || !pendingFitRef.current) return;
    pendingFitRef.current = false;
    controllerRef.current?.fit();
    controllerRef.current?.syncSize();
  }, [holdingSize]);

  // Names the terminal's input in the UI language; it follows a language change in place, as
  // xterm only names the input once, when it opens.
  const inputLabel = t("terminal.input");
  useEffect(() => {
    controllerRef.current?.setInputLabel(inputLabel);
  }, [inputLabel]);

  // Recolours the running terminal in place rather than recreating it — the instance above is
  // mount-only, and `setColorTheme` is exactly the escape hatch for a change that must still reach
  // it. Skipped when `colorTheme` already matches what the controller has: without this check, the
  // constructor's own initial theme and this effect's first run (which land in the same task as
  // `mount()`'s `term.open()`) would apply the identical colours twice, the second time via
  // `setColorTheme`, which drives a theme-service and renderer refresh — the same class of operation
  // `TerminalController.mount()` defers `fit`/`reset` around because xterm throws from its own
  // scheduled work when they run in that task.
  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller || colorTheme === appliedColorThemeRef.current) return;
    appliedColorThemeRef.current = colorTheme;
    controller.setColorTheme(colorTheme);
  }, [colorTheme]);
}
