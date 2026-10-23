import { Label, ProgressBar } from "@heroui/react";
import React, { useEffect, useId, useRef, useState } from "react";

import { SharingCauseLine, SharingCauseSummary, SharingDetails, SharingFailureReason } from "../components/DaemonErrorMessage";
import { ErrorAlert } from "../components/ErrorAlert";
import { HelpTip } from "../components/HelpTip";
import { InlineChip } from "../components/InlineChip";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { parseSharingItems, sharingCauseGroups, sharingFixKey, sharingReasonKey } from "../daemonMessage";
import { errorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { Account, AccountSharing } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { SettingSwitch } from "./SettingRow";

/** An account's Share default configuration switch (see "The sharing choice" in
 * `docs/product/sharing-the-default-configuration.md`), shown for every account but the agent's default one. A
 * change is carried out at once, so the switch is either free or unavailable, and says which. Unavailable, it stays
 * focusable and describes itself, so a keyboard user hears why (see "Buttons" in `docs/ui-accessibility.md`):
 * - free: a caption of what turning it on does, or of what is the case now;
 * - unavailable while sessions of the account run or start under its directory, or it is being signed in
 *   (`in_use`), saying so;
 * - unavailable while the change runs (`progress`), under a progress bar that every window shows from the
 *   daemon's `account_sharing` event;
 * - after a failure, or a change the daemon restarted in the middle of (`share_default_config_incomplete`), one compact
 *   warning saying what did not finish and, for a failure, two sentences: with a single cause, that cause's own line
 *   and its fix; with several, that some files or folders could not be changed and the way to try again (turning the
 *   switch). Behind its Details button are the line of each cause with its fix (when there are several), the way to try
 *   again (when the fix took its place), the note on idle sessions and the raw diagnostics.
 *   The switch then shows the position it had before the change, so turning it is asking for the recorded choice once
 *   more, which the daemon treats as the retry (but for a default directory that moved, which only turning sharing
 *   off and on again mends; the switch then shows the recorded choice). The warning describes the switch in place of
 *   its caption. Focus inside it goes to the switch when it goes. */
export function AccountSharingSwitch({ account }: { account: Account }): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const sharing = useDaemonStore((s) => s.accountSharing.get(account.id));
  const [busy, setBusy] = useState(false);
  const id = useId();
  const label = t("settings.accounts.sharing.switchLabel", { name: isolate(account.name) });
  const shared = account.share_default_config === true;
  const progress = sharing?.progress;
  const idle = sharing?.idle_sessions ?? 0;
  const inUse = sharing?.in_use && (sharing.in_use.sessions > 0 || sharing.in_use.signing_in) ? sharing.in_use : undefined;
  // A change that did not finish: the daemon says so with the flag, and may hold why (`failure`, until the next change
  // starts); after a restart there is the flag alone.
  const unfinished = !progress && account.share_default_config_incomplete === true;
  const failure = unfinished ? sharing?.failure : undefined;
  const moved = failure?.cause === "default_moved";
  const locked = busy || progress !== undefined || inUse !== undefined;
  // A retry is turning the switch, which asks for the recorded choice again; so the switch shows the position it had
  // before. A moved default directory cannot be mended by asking again, so it shows the recorded choice.
  const shown = unfinished && !moved && !busy ? !shared : shared;
  const guidance = moved ? "settings.accounts.sharing.guidanceMoved" : shared ? "settings.accounts.sharing.guidanceOn" : "settings.accounts.sharing.guidanceOff";
  // Only a failure that follows a change this view saw run (its own request, or progress from the daemon) is announced:
  // one already there when it mounted, or restated by an unrelated update, is not.
  const [sawChange, setSawChange] = useState(false);
  const running = progress !== undefined || busy;
  if (running && !sawChange) setSawChange(true);
  else if (!running && failure === undefined && sawChange) setSawChange(false);
  const announce = failure !== undefined && sawChange;
  const root = useRef<HTMLDivElement>(null);
  const focusInWarning = useRef(false);
  useEffect(() => {
    if (unfinished || !focusInWarning.current) return;
    focusInWarning.current = false;
    // Only when focus went with the warning; a user who has moved on is left where they are.
    if (document.activeElement && document.activeElement !== document.body) return;
    root.current?.querySelector<HTMLElement>('input[role="switch"]')?.focus();
  }, [unfinished]);
  // Asks for `share`; the request is answered when the change ends. The failure of the change itself is shown at the
  // switch, so only a refusal is reported as an error alert.
  const ask = (share: boolean) => {
    if (locked) return;
    setBusy(true);
    void request({ type: "set_account_sharing", account: account.id, share })
      .catch((err) => {
        if ((err as { code?: string }).code !== "account_sharing_failed") toastError(errorPresentation(err as Error));
      })
      .finally(() => setBusy(false));
  };
  const caption = inUse
    ? <>
      {inUse.sessions > 0 && <span className="block">{t("settings.accounts.sharing.inUseSessions", { count: inUse.sessions })}</span>}
      {inUse.signing_in && <span className="block">{t("settings.accounts.sharing.inUseSigningIn")}</span>}
    </>
    : unfinished || progress ? undefined : (
      <>
        {t(shared ? "settings.accounts.sharing.describeOn" : "settings.accounts.sharing.describeOff")}
        {idle > 0 && <span className="block">{t("settings.accounts.sharing.idleSessions", { count: idle })}</span>}
      </>
    );
  const describedBy = [
    caption !== undefined && `${id}-caption`,
    progress && `${id}-label`,
    progress && `${id}-value`,
    progress && progress.phase === "changing" && progress.item !== undefined && `${id}-current`,
    unfinished && `${id}-title`,
    unfinished && failure && `${id}-reason`,
    unfinished && `${id}-remedy`,
  ].filter(Boolean).join(" ") || undefined;
  return (
    <div ref={root} data-sharing-account={account.id} className="col-span-full">
      <div className="flex items-center gap-2 text-sm">
        <span className="font-medium">{t("settings.accounts.sharing.label")}</span>
        <HelpTip label={t("settings.accounts.sharing.helpLabel")}>
          <div className="flex flex-col gap-2">
            <p>{t("settings.accounts.sharing.helpShared")}</p>
            <p>
              {account.agent === "claude"
                ? <Message id="settings.accounts.sharing.helpNeverClaude" params={{
                  claudeJson: <InlineChip dir="ltr" monospace wrap unbroken>.claude.json</InlineChip>,
                  plans: <InlineChip dir="ltr" monospace wrap unbroken>plans/</InlineChip>,
                }} />
                : account.agent === "grok"
                  ? <Message id="settings.accounts.sharing.helpNeverGrok" params={{
                    trustedFolders: <InlineChip dir="ltr" monospace wrap unbroken>trusted_folders.toml</InlineChip>,
                    hooksPaths: <InlineChip dir="ltr" monospace wrap unbroken>hooks-paths</InlineChip>,
                  }} />
                  : t("settings.accounts.sharing.helpNever")}
            </p>
            <p><Message id="settings.accounts.sharing.helpOff" /></p>
          </div>
        </HelpTip>
        <div className="ms-auto">
          <SettingSwitch label={label} isSelected={shown} onChange={ask} isUnavailable={locked} describedBy={describedBy} />
        </div>
      </div>
      {caption !== undefined && <p id={`${id}-caption`} className="text-sm text-app-muted">{caption}</p>}
      {progress && <SharingProgress id={id} progress={progress} />}
      {unfinished && (
        <div onFocus={() => { focusInWarning.current = true; }} onBlur={(event) => { if (event.relatedTarget) focusInWarning.current = false; }}>
          <SharingIncomplete id={id} failure={failure} share={shared} guidance={guidance} announce={announce} idle={idle} />
        </div>
      )}
    </div>
  );
}

/** A running change: a bar of the entries done out of the total with the entry being processed under it, or an
 * indeterminate one while the daemon waits for short background reads. The bar is a progress bar named by the
 * direction, with its value read as the count (or, indeterminate, described by the wait); the direction is also
 * spoken once when the change starts. */
function SharingProgress({ id, progress }: { id: string; progress: NonNullable<AccountSharing["progress"]> }): React.ReactElement {
  const t = useT();
  const label = t(progress.share ? "settings.accounts.sharing.progressOn" : "settings.accounts.sharing.progressOff");
  // Only the entries phase counts; the others, and the start of that one, wait on something with no count to give.
  const entries = progress.phase === "changing" && progress.item !== undefined;
  const wait = t(
    progress.phase === "waiting" ? "settings.accounts.sharing.phaseWaiting"
      : progress.phase === "stopping" ? "settings.accounts.sharing.phaseStopping"
        : progress.phase === "resuming" ? "settings.accounts.sharing.phaseResuming"
          : "settings.accounts.sharing.waiting",
  );
  const count = t("settings.accounts.sharing.progressCount", { done: progress.done, total: progress.total });
  // Speaks again for ending sessions, waiting and resuming; entering the changing phase keeps what was said.
  const [spoken, setSpoken] = useState<string>();
  const next = progress.phase === "changing" ? spoken ?? label : `${label}. ${wait}`;
  if (next !== spoken) setSpoken(next);
  return (
    <div className="mt-2 flex min-w-0 flex-col gap-2 text-sm">
      <StatusAnnouncer text={next} />
      <ProgressBar
        value={progress.done}
        maxValue={Math.max(progress.total, 1)}
        isIndeterminate={!entries}
        valueLabel={count}
        aria-describedby={entries ? undefined : `${id}-value`}
      >
        <div className="flex items-baseline justify-between gap-2">
          <span id={`${id}-label`}><Label>{label}</Label></span>
          <span id={`${id}-value`} className="text-app-muted">{entries ? count : wait}</span>
        </div>
        <ProgressBar.Track><ProgressBar.Fill /></ProgressBar.Track>
      </ProgressBar>
      {entries && (
        <p id={`${id}-current`} className="[overflow-wrap:anywhere]">
          <Message id="settings.accounts.sharing.current" params={{ item: <InlineChip dir="ltr" monospace wrap unbroken>{progress.item}</InlineChip> }} />
        </p>
      )}
    </div>
  );
}

/** The change that did not finish, whether it failed just now or the daemon restarted in the middle of it. One compact
 * warning: the title and, for a failure, two sentences, in the one live region that is all a screen reader is told when
 * it appears (the remedy, under it and not announced, is described by the switch too). With a single cause the
 * sentences are that cause's own line and its fix, or, where the cause has no fix of its own (`not_found`, a default
 * directory that moved), the way to try again; with several, that some files or folders could not be changed and the
 * way to try again (turning the switch, so there is no retry button). Behind its Details button, which opens a popover,
 * are first the further explanation: each cause's line with its fix when there are several, the way to try again when
 * the fix took its place, and the note on idle sessions, then the raw entries and the system's own reason; with none of
 * it there is no Details button. */
function SharingIncomplete({ id, failure, share, guidance, announce, idle }: {
  id: string;
  idle: number;
  failure: AccountSharing["failure"];
  share: boolean;
  guidance: "settings.accounts.sharing.guidanceOn" | "settings.accounts.sharing.guidanceOff" | "settings.accounts.sharing.guidanceMoved";
  announce: boolean;
}): React.ReactElement {
  const t = useT();
  const items = parseSharingItems(failure?.items);
  const groups = items?.length ? sharingCauseGroups(items) : undefined;
  // A single cause is said by itself: one named group of entries, or the change's own cause with no entry to name.
  const group = groups?.length === 1 ? groups[0] : undefined;
  const whole = !groups && failure && sharingReasonKey(failure.cause) ? failure.cause : undefined;
  const single = group !== undefined || whole !== undefined;
  const fix = group ? sharingFixKey(group.cause) : sharingFixKey(whole);
  const guided = <span>{t(guidance)}</span>;
  // With several causes, a cause with no line of its own points at the raw reason, which is right below it here, so it
  // says nothing; a single cause (a group or the change's own) is said by the guidance and fix below, not here.
  const reason = single || !groups ? undefined : <SharingCauseSummary items={items!} />;
  const raw = failure && (items?.length || failure.detail) ? <SharingDetails items={items ?? []} detail={failure.detail} folded={false} /> : undefined;
  const note = idle > 0 ? <span>{t("settings.accounts.sharing.idleSessions", { count: idle })}</span> : undefined;
  const later = single && fix ? <span className="block">{t(guidance)}</span> : undefined;
  const details = reason || later || note || raw ? <>{reason}{later}{note}{raw}</> : undefined;
  return (
    <ErrorAlert
      status="warning"
      className="mt-2 w-full"
      announce={announce}
      title={<span id={`${id}-title`}>{t(failure ? (share ? "settings.accounts.sharing.failedOn" : "settings.accounts.sharing.failedOff") : (share ? "settings.accounts.sharing.incompleteOn" : "settings.accounts.sharing.incompleteOff"))}</span>}
      remedy={<span id={`${id}-remedy`}>{single && fix ? <Message id={fix} /> : guided}</span>}
      details={details}
    >
      {failure && <span id={`${id}-reason`}>{group ? <SharingCauseLine group={group} withFix={false} /> : whole ? <SharingFailureReason cause={whole} withFix={false} /> : t("settings.accounts.sharing.failedSummary")}</span>}
    </ErrorAlert>
  );
}
