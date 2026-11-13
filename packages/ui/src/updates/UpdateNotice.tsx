import { Download, RefreshCw, X } from "lucide-react";
import React from "react";

import { CopyButton } from "../components/controls/CopyButton";
import { AttendedStatusAnnouncer } from "../components/announce/AttendedStatusAnnouncer";
import { ExternalLink } from "../components/text/ExternalLink";
import { TitledControl } from "../components/hint/TitledControl";
import { useBottomStrip } from "../components/panel/useBottomStrip";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import { useHost } from "../platform/react";
import { installedMessageKey } from "./installedMessageKey";
import { reloadPage } from "./reloadPage";
import { releaseUrl } from "../components/text/releaseUrl";
import { updateNoticeKind } from "./noticeKind";
import { useUpdates } from "./UpdatesProvider";
import { Button, Label, ProgressBar, ProgressBarOutput, ProgressBarTrack } from "../primitives";

/** The copy of each notice but an installed update's, which depends on how the platform applies it. */
const NOTICE_KEYS = {
  reload: "updates.notice.reloadNow",
  manual: "updates.notice.manualInstall",
  downloading: "updates.notice.downloading",
  available: "updates.notice.available",
} as const;

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
 * updates without taking the window: a newer version is available (with a link to its release page, and
 * a Download and install that is the user's to press, never done automatically), the download is under way, or the
 * update is installed and takes effect after a restart (Restart now, or Later, which only hides the strip). Where an
 * installer applies the update (Windows) it only says the update is downloaded, and installed when the application
 * quits or restarts (`installedMessageKey`). For a daemon that cannot replace its own executable it says so, with the
 * install command to copy and no download; and a page whose daemon now runs another version is asked to reload (a
 * Reload that is the user's to press, never done automatically). Nothing here restarts anything: Restart now goes
 * through the window's exit confirmation (`UpdateStore.restart`).
 *
 * Mounted whatever the update does, so the announcement region is not inserted already holding its text; the strip
 * is a stop of the F6 region cycle (`data-region="update"`) while shown. A button pressed there that makes the strip
 * change or go away would drop keyboard focus to `<body>`, so focus is handed to an open dialog, otherwise to the
 * terminal (`useBottomStrip`), which also publishes the strip's height (`NOTICE_HEIGHT_VAR`), since its text decides it.
 *
 * What it says is announced once the window is attended (`AttendedStatusAnnouncer`): the six-hourly check and a
 * finished install both land while the user may be elsewhere.
 */
export function UpdateNotice({ focusTerminal }: { focusTerminal: () => void }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const updates = useUpdates();
  const host = useHost();
  const kind = updates ? updateNoticeKind(updates.state) : undefined;
  const { ref, focusProps } = useBottomStrip({ visible: kind !== undefined, heightVar: NOTICE_HEIGHT_VAR, focusTerminal });

  if (!updates) return <></>;
  const { store, state } = updates;
  const version = isolate(state.update?.version ?? state.installedVersion ?? "");
  const message =
    kind &&
    (kind === "installed"
      ? t(installedMessageKey(host.updater, "notice", !!state.installedVersion), { version })
      : t(NOTICE_KEYS[kind], { version: kind === "reload" ? isolate(state.currentVersion ?? "") : version }));
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
            {kind === "downloading" ? (
              <>
                <Download aria-hidden="true" className="size-3.5 shrink-0" />
                {/* The bar's colours follow the strip's own foreground: its default accent fill and neutral track
                  would vanish on the accent fill, and its label would take the page's text colour. */}
                <ProgressBar
                  className="flex min-w-0 max-w-96 flex-1 items-center gap-2 text-accent-foreground [--progress-bar-fill:var(--accent-foreground)]"
                  value={percent === undefined ? undefined : percent * 100}
                  indeterminate={percent === undefined}
                  valueLabel={percentText}
                >
                  <Label className="shrink-0 whitespace-nowrap text-xs text-accent-foreground">{message}</Label>
                  {percentText && <ProgressBarOutput className="order-last shrink-0 text-xs text-accent-foreground" />}
                  <ProgressBarTrack className="min-w-8 flex-1 bg-accent-foreground/25" />
                </ProgressBar>
              </>
            ) : (
              // Centred, and balanced when it wraps; the glyph stays with the first line, as part of the text.
              <span className="text-center text-balance">
                {kind === "reload" ? <RefreshCw aria-hidden="true" className="me-2 inline size-3.5 align-text-bottom" /> : <Download aria-hidden="true" className="me-2 inline size-3.5 align-text-bottom" />}
                {message}
              </span>
            )}
            {(kind === "available" || kind === "manual") && state.update && (
              <ExternalLink href={releaseUrl(state.update.version)} appearance="button" variant="outline" size="xs" className={BUTTON_CLASS}>
                {t("updates.notice.notes")}
              </ExternalLink>
            )}
            {kind === "manual" && state.manual && (
              <CopyButton text={state.manual.command} failedText={t("updates.notice.copyCommandFailed")} size="xs" variant="outline" className={BUTTON_CLASS} focusOnPress={false}>
                {t("updates.notice.copyCommand")}
              </CopyButton>
            )}
            {kind === "reload" && (
              <Button size="xs" variant="outline" className={BUTTON_CLASS} focusOnPress={false} onPress={reloadPage}>
                {t("updates.notice.reloadAction")}
              </Button>
            )}
            {(kind === "available" || kind === "manual") && (
              <>
                {kind === "available" && (
                  <Button size="xs" variant="outline" className={BUTTON_CLASS} focusOnPress={false} onPress={() => void store.install()}>
                    {t("updates.notice.install")}
                  </Button>
                )}
                <TitledControl title={t("updates.notice.dismiss")}>
                  <Button size="xs" variant="ghost" iconOnly className={BUTTON_CLASS} label={t("updates.notice.dismiss")} focusOnPress={false} onPress={store.dismissNotice}>
                    <X aria-hidden="true" className="size-3.5" />
                  </Button>
                </TitledControl>
              </>
            )}
            {kind === "installed" && (
              <>
                <Button size="xs" variant="outline" className={BUTTON_CLASS} focusOnPress={false} pending={state.restarting} onPress={() => void store.restart()}>
                  {t("updates.notice.restart")}
                </Button>
                <Button size="xs" variant="outline" className={BUTTON_CLASS} focusOnPress={false} onPress={store.hideRestartNotice}>
                  {t("updates.notice.later")}
                </Button>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
