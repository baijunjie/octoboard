import { Label, ProgressBar } from "@heroui/react";
import React, { useId, useRef, useState } from "react";

import { defaultAccountName } from "../agents";
import { SharingCauseLine, SharingCauseSummary, SharingDetails, SharingFailureReason } from "../components/SharingFailure";
import { HandOffOnUnmount } from "../components/HandOffOnUnmount";
import { HelpTip } from "../components/HelpTip";
import { InlineReference } from "../components/InlineReference";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { StatusMark } from "../components/StatusMark";
import { parseSharingItems, sharingCauseGroups, sharingFixKey, sharingReasonKey } from "../daemonMessage";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { Account, AccountSharing } from "../protocol";
import { reportError } from "../reportError";
import { useDaemon, useDaemonStore } from "../store";
import { SettingRow, SettingSwitch } from "./SettingRow";

/** An account's Share default configuration switch (see "The sharing choice" in
 * `docs/product/sharing-the-default-configuration.md`), shown for every account but the agent's default one, in the
 * account's settings dialog, as a `SettingRow` (label with its help tip and a description, the switch at the end,
 * with the warning's mark after it): the progress bar takes the row's full width under it. What the switch does now
 * is the row's description, following its position (and left out while a change runs or did not finish); what
 * turning it off does is behind the label's help tip, with the idle sessions it would end; and under the
 * description, why the switch is locked. A change is carried out at once, so the switch is either free or
 * unavailable, and says which. Unavailable, it stays focusable and describes itself, so a keyboard user hears why
 * (see "Buttons" in `docs/ui-accessibility.md`):
 * - free: nothing under it;
 * - unavailable while sessions of the account run or start under its directory, or it is being signed in (`in_use`),
 *   saying so;
 * - unavailable while the change runs (`progress`), under a progress bar that every window shows from the daemon's
 *   `account_sharing` event;
 * - after a failure, or a change the daemon restarted in the middle of (`share_default_config_incomplete`), a warning
 *   mark right after the switch, named "Last sharing change", with nothing written beside it. Its popover says what
 *   did not finish and, for a failure, two sentences: with a single cause, that cause's own line and its fix; with
 *   several, that some files or folders could not be changed and the way to try again (turning the switch). Under
 *   them are the line of each cause with its fix (when there are several), the way to try again (when the fix took its
 *   place), the note on idle sessions and the raw diagnostics.
 *   The switch then shows the position it had before the change, so turning it is asking for the recorded choice once
 *   more, which the daemon treats as the retry (but for a default directory that moved, which only turning sharing off
 *   and on again mends; the switch then shows the recorded choice). The switch is described by the title in place of
 *   its caption (a hidden copy, since the popover is not there while closed); the title is spoken when the mark appears
 *   after a change this view saw, unless this window asked for it: the error alert dialog that opens then speaks it.
 *   Focus on the mark or in its popover goes to the switch when the mark goes. */
export function AccountSharingSwitch({ account, opensFocused }: { account: Account; opensFocused?: boolean }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const sharing = useDaemonStore((s) => s.accountSharing.get(account.id));
  const defaultName = useDaemonStore((s) => defaultAccountName(t, s.settings.default_account_names, account.agent));
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
  const title = t(failure
    ? (shared ? "settings.accounts.sharing.failedOn" : "settings.accounts.sharing.failedOff")
    : (shared ? "settings.accounts.sharing.incompleteOn" : "settings.accounts.sharing.incompleteOff"));
  const guidance = moved ? "settings.accounts.sharing.guidanceMoved" : shared ? "settings.accounts.sharing.guidanceOn" : "settings.accounts.sharing.guidanceOff";
  // Only a failure that follows a change this view saw run (its own request, or progress from the daemon) is announced:
  // one already there when it mounted, or restated by an unrelated update, is not. Nor is one that follows a change
  // asked for here: the error alert dialog opened for it speaks it, and the mark is its lasting state. Whether the
  // change was asked for here is decided per run, as the run starts: a run that shows up without this window's own
  // request (another window's) is not.
  const [sawChange, setSawChange] = useState(false);
  const [askedHere, setAskedHere] = useState(false);
  const running = progress !== undefined || busy;
  const [wasRunning, setWasRunning] = useState(false);
  if (running !== wasRunning) {
    setWasRunning(running);
    if (running && !busy) setAskedHere(false);
  }
  if (running && !sawChange) setSawChange(true);
  else if (!running && failure === undefined && sawChange) {
    setSawChange(false);
    setAskedHere(false);
  }
  const announce = failure !== undefined && sawChange && !askedHere;
  const root = useRef<HTMLDivElement>(null);
  // Asks for `share`; the request is answered when the change ends. Whatever it is answered with besides success, a
  // refusal or a change that ended unfinished, is reported as an error alert in this window, which asked for it. A
  // change that ended unfinished also leaves the mark at the switch, as the same change seen from another window does
  // alone.
  const ask = (share: boolean) => {
    if (locked) return;
    setBusy(true);
    setAskedHere(true);
    void request({ type: "set_account_sharing", account: account.id, share })
      .catch((err) => reportError(err, undefined, undefined, undefined, t("settings.accounts.sharing.failedTitle")))
      .finally(() => setBusy(false));
  };
  // The only line kept under the switch: why it cannot be turned now. What it does is in the help tip.
  const caption = inUse
    ? <>
      {inUse.sessions > 0 && <span className="block">{t("settings.accounts.sharing.inUseSessions", { count: inUse.sessions })}</span>}
      {inUse.signing_in && <span className="block">{t("settings.accounts.sharing.inUseSigningIn")}</span>}
    </>
    : undefined;
  const describedBy = [
    caption !== undefined && `${id}-caption`,
    progress && `${id}-label`,
    progress && `${id}-value`,
    progress && progress.phase === "changing" && progress.item !== undefined && `${id}-current`,
    unfinished && `${id}-title`,
  ].filter(Boolean).join(" ") || undefined;
  return (
    <SettingRow
      label={t("settings.accounts.sharing.label")}
      help={
        <HelpTip label={t("settings.accounts.sharing.helpLabel")}>
          <div className="flex flex-col gap-2">
            <p>{t("settings.accounts.sharing.helpShared")}</p>
            <p>
              {account.agent === "claude"
                ? <Message id="settings.accounts.sharing.helpNeverClaude" params={{
                  claudeJson: <InlineReference dir="ltr" monospace wrap unbroken>.claude.json</InlineReference>,
                  plans: <InlineReference dir="ltr" monospace wrap unbroken>plans/</InlineReference>,
                }} />
                : account.agent === "grok"
                  ? <Message id="settings.accounts.sharing.helpNeverGrok" params={{
                    trustedFolders: <InlineReference dir="ltr" monospace wrap unbroken>trusted_folders.toml</InlineReference>,
                    hooksPaths: <InlineReference dir="ltr" monospace wrap unbroken>hooks-paths</InlineReference>,
                  }} />
                  : t("settings.accounts.sharing.helpNever")}
            </p>
            <p><Message id="settings.accounts.sharing.helpOff" /></p>
            {idle > 0 && <p>{t("settings.accounts.sharing.idleSessions", { count: idle })}</p>}
          </div>
        </HelpTip>
      }
      description={
        <>
          {!progress && !unfinished && <Message
            id={shared ? "settings.accounts.sharing.describeOnAccount" : "settings.accounts.sharing.describeOffAccount"}
            params={{ name: <InlineReference dir="auto" wrap>{defaultName}</InlineReference> }}
          />}
          {caption !== undefined && <p id={`${id}-caption`} className="text-xs">{caption}</p>}
        </>
      }
      below={progress && <SharingProgress id={id} progress={progress} />}
    >
      <div className="flex items-center gap-2">
        <div ref={root} data-sharing-account={account.id}>
          <SettingSwitch label={label} isSelected={shown} onChange={ask} isUnavailable={locked} describedBy={describedBy} opensFocused={opensFocused} />
        </div>
        {unfinished && (
          <SharingIncomplete
            id={id}
            failure={failure}
            title={title}
            guidance={guidance}
            idle={idle}
            focusSwitch={() => root.current?.querySelector<HTMLElement>('input[role="switch"]')?.focus()}
          />
        )}
        <StatusAnnouncer text={announce ? title : undefined} clearAfterRead />
      </div>
    </SettingRow>
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
  const next = progress.phase === "changing" ? spoken ?? label : t("settings.accounts.sharing.progressWithWait", { progress: label, wait });
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
          <Message id="settings.accounts.sharing.current" params={{ item: <InlineReference dir="ltr" monospace wrap unbroken>{progress.item}</InlineReference> }} />
        </p>
      )}
    </div>
  );
}

/** The change that did not finish, whether it failed just now or the daemon restarted in the middle of it: a warning
 * mark in the row's control area, after the switch, with everything of it in its popover. In order: the title; for a
 * failure, the reason and either its fix or the way to try again (turning the switch, so there is no retry button);
 * then the several causes' lines, the way to try again where the fix took its place, the idle-sessions note and the raw
 * entries. With a single cause the reason is that cause's own line and its fix, or, where the cause has no fix of its
 * own (`not_found`, a default directory that moved), the way to try again; with several, that some files or folders
 * could not be changed and the way to try again. The switch is described by a hidden copy of the title (`${id}-title`);
 * the title is spoken by the switch's own announcer. The mark takes the focus it or its popover held to the switch as
 * it goes. */
function SharingIncomplete({ id, failure, title, guidance, idle, focusSwitch }: {
  id: string;
  idle: number;
  failure: AccountSharing["failure"];
  title: string;
  guidance: "settings.accounts.sharing.guidanceOn" | "settings.accounts.sharing.guidanceOff" | "settings.accounts.sharing.guidanceMoved";
  focusSwitch: () => void;
}): React.ReactElement {
  const t = useT();
  const items = parseSharingItems(failure?.items);
  const groups = items?.length ? sharingCauseGroups(items) : undefined;
  // A single cause is said by itself: one named group of entries, or the change's own cause with no entry to name.
  const group = groups?.length === 1 ? groups[0] : undefined;
  const whole = !groups && failure && sharingReasonKey(failure.cause) ? failure.cause : undefined;
  const single = group !== undefined || whole !== undefined;
  // The raw reason is further down the same popover, so a fix that points at it says so.
  const fix = sharingFixKey(group ? group.cause : whole, true);
  // With several causes, a cause with no line of its own points at the raw reason, which is right below it here, so it
  // says nothing; a single cause (a group or the change's own) is said by the guidance and fix below, not here.
  const reason = single || !groups ? undefined : <SharingCauseSummary items={items!} rawBelow />;
  const raw = failure && (items?.length || failure.detail) ? <SharingDetails items={items ?? []} detail={failure.detail} /> : undefined;
  const markId = `${id}-mark`;
  return (
    <>
      {/* The mark goes with the warning; focus it or its popover held goes to the switch. */}
      <HandOffOnUnmount from={`[data-status-mark-id="${markId}"]`} to={focusSwitch} />
      <StatusMark tone="warning" markId={markId} label={t("settings.accounts.sharing.unfinishedMark")}>
        <p>{title}</p>
        {failure && <p>{group ? <SharingCauseLine group={group} withFix={false} /> : whole ? <SharingFailureReason cause={whole} withFix={false} /> : t("settings.accounts.sharing.failedSummary")}</p>}
        <p>{single && fix ? <Message id={fix} /> : t(guidance)}</p>
        {reason}
        {single && fix && <p>{t(guidance)}</p>}
        {idle > 0 && <p>{t("settings.accounts.sharing.idleSessions", { count: idle })}</p>}
        {raw}
      </StatusMark>
      <span id={`${id}-title`} className="sr-only">{title}</span>
    </>
  );
}
