import { Label, ProgressBar } from "@heroui/react";
import { ChevronDown, Download, X } from "lucide-react";
import React, { useId, useState } from "react";

import { AttendedStatusAnnouncer } from "../components/AttendedStatusAnnouncer";
import { Button } from "../components/Button";
import { FadedScroller, useFadeRegion } from "../components/scrollFade";
import { TitledControl } from "../components/TitledControl";
import { useBottomStrip } from "../components/useBottomStrip";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import { updateNoticeKind } from "./noticeKind";
import { useUpdates } from "./UpdatesProvider";

/** The CSS custom property this notice publishes its own height to, which `--bottom-chrome-height` in `style.css`
 * adds to the connection banner's, so what overlays the content area ends above it. */
const NOTICE_HEIGHT_VAR = "--update-notice-height";

/** HeroUI's outline button retinted through the button's own color tokens to sit on the notice's solid accent
 * fill. */
const BUTTON_CLASS =
  "rounded-lg border-accent-foreground/30 " +
  "[--button-bg-hover:var(--accent-hover)] [--button-bg-pressed:var(--accent-hover)] " +
  "[--button-fg:var(--accent-foreground)]";

/**
 * The strip along the bottom edge, above the connection banner, that tells the user about the application's
 * updates without taking the window: a newer version is available (with its release notes behind a disclosure, and
 * a Download and install that is the user's to press, never done automatically), the download is under way, or the
 * update is installed and takes effect after a restart (Restart now, or Later, which only hides the strip). Nothing
 * here restarts anything: Restart now goes through the window's exit confirmation (`UpdateStore.restart`).
 *
 * Mounted whatever the update does, so the announcement region is not inserted already holding its text; the strip
 * is a stop of the F6 region cycle (`data-region="update"`) while shown. A button pressed there that makes the strip
 * change or go away would drop keyboard focus to `<body>`, so focus is handed to an open dialog, otherwise to the
 * terminal (`useBottomStrip`), which also publishes the strip's height (`NOTICE_HEIGHT_VAR`), since the text and the
 * notes decide it.
 *
 * What it says is announced once the window is attended (`AttendedStatusAnnouncer`): the six-hourly check and a
 * finished install both land while the user may be elsewhere.
 */
export function UpdateNotice({ focusTerminal }: { focusTerminal: () => void }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const updates = useUpdates();
  const notesId = useId();
  const [notesOpen, setNotesOpen] = useState(false);
  const kind = updates ? updateNoticeKind(updates.state) : undefined;
  const { ref, focusProps } = useBottomStrip({ visible: kind !== undefined, heightVar: NOTICE_HEIGHT_VAR, focusTerminal });

  if (!updates) return <></>;
  const { store, state } = updates;
  const version = isolate(state.update?.version ?? state.installedVersion ?? "");
  const message = kind && t(kind === "installed" && !state.installedVersion ? "updates.notice.installedUnknown" : `updates.notice.${kind}`, { version });
  const notes = kind === "available" ? state.update?.notes : undefined;
  const progress = state.progress;
  const percent = progress?.total ? Math.min(progress.downloaded / progress.total, 1) : undefined;
  const percentText = percent === undefined ? undefined : new Intl.NumberFormat(language, { style: "percent" }).format(percent);

  return (
    <div className="flex-none">
      <AttendedStatusAnnouncer text={message} />
      {kind && (
        <div
          ref={ref}
          role="region"
          aria-label={t("updates.notice.label")}
          data-region="update"
          {...focusProps}
          className="flex flex-col gap-1.5 bg-accent px-3 py-1.5 text-xs text-accent-foreground"
        >
          <div className={`flex min-h-6 items-center justify-center gap-x-3 gap-y-1 ${kind === "downloading" ? "" : "flex-wrap"}`}>
            <Download aria-hidden="true" className="size-3.5 shrink-0" />
            {kind === "downloading" ? (
              // The bar's colours follow the strip's own foreground: its default accent fill and neutral track
              // would vanish on the accent fill, and its label would take the page's text colour.
              <ProgressBar
                className="flex min-w-0 max-w-96 flex-1 items-center gap-2 text-accent-foreground [--progress-bar-fill:var(--accent-foreground)]"
                value={percent === undefined ? undefined : percent * 100}
                isIndeterminate={percent === undefined}
                valueLabel={percentText}
              >
                <Label className="shrink-0 whitespace-nowrap text-xs text-accent-foreground">{message}</Label>
                {percentText && <ProgressBar.Output className="order-last shrink-0 text-xs text-accent-foreground" />}
                <ProgressBar.Track className="min-w-8 flex-1 bg-accent-foreground/25"><ProgressBar.Fill /></ProgressBar.Track>
              </ProgressBar>
            ) : (
              <span>{message}</span>
            )}
            {notes && (
              <Button size="xs" variant="outline" className={BUTTON_CLASS} preventFocusOnPress aria-expanded={notesOpen} aria-controls={notesId} onPress={() => setNotesOpen(!notesOpen)}>
                {t("updates.notice.notes")}
                <ChevronDown aria-hidden="true" className={`size-3 transition-transform motion-reduce:transition-none ${notesOpen ? "rotate-180" : ""}`} />
              </Button>
            )}
            {kind === "available" && (
              <>
                <Button size="xs" variant="outline" className={BUTTON_CLASS} preventFocusOnPress onPress={() => void store.install()}>
                  {t("updates.notice.install")}
                </Button>
                <TitledControl title={t("updates.notice.dismiss")}>
                  <Button size="xs" variant="ghost" isIconOnly className={BUTTON_CLASS} aria-label={t("updates.notice.dismiss")} preventFocusOnPress onPress={store.dismissNotice}>
                    <X aria-hidden="true" className="size-3.5" />
                  </Button>
                </TitledControl>
              </>
            )}
            {kind === "installed" && (
              <>
                <Button size="xs" variant="outline" className={BUTTON_CLASS} preventFocusOnPress onPress={store.restart}>
                  {t("updates.notice.restart")}
                </Button>
                <Button size="xs" variant="outline" className={BUTTON_CLASS} preventFocusOnPress onPress={store.hideRestartNotice}>
                  {t("updates.notice.later")}
                </Button>
              </>
            )}
          </div>
          {notes && notesOpen && (
            <Notes id={notesId}>{notes}</Notes>
          )}
        </div>
      )}
    </div>
  );
}

/** The release notes: a named region that scrolls, and is a tab stop only while it overflows, with the fade at its edges. */
function Notes({ id, children }: { id: string; children: string }): React.ReactElement {
  const t = useT();
  const region = useFadeRegion({ label: t("updates.notice.notes") });
  return (
    <FadedScroller id={id} region={region} className="max-h-32 overflow-y-auto whitespace-pre-wrap rounded-lg bg-accent-foreground/10 px-3 py-2 text-start [overflow-wrap:anywhere]">
      {children}
    </FadedScroller>
  );
}
