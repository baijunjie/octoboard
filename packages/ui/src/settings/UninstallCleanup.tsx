import { CircleAlert, CircleCheck, TriangleAlert } from "lucide-react";
import React, { useCallback, useContext, useEffect, useId, useRef, useState } from "react";

import { providerLabel } from "../agents/providers";
import { DetailsDisclosure } from "../components/alerts/ErrorDetailsDisclosure";
import { DiagnosticBlock } from "../components/alerts/DiagnosticBlock";
import { ErrorAlert } from "../components/alerts/ErrorAlert";
import { useFailureNode } from "../components/alerts/failureNode";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { useWindowAttended } from "../components/announce/useWindowAttended";
import { CopyCommand } from "../components/controls/CopyBox";
import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { Dialog } from "../components/dialog/Dialog";
import { dialogAround } from "../components/dialog/dialogElement";
import { useFocusWhenShown } from "../components/dialog/dialogHooks";
import { TextDisclosure } from "../components/disclosure/TextDisclosure";
import { afterFrames } from "../components/focus/afterFrames";
import { useHandoffAfterRemoval } from "../components/focus/HandoffOnUnmount";
import { useFocusReturnAfterClose } from "../components/focus/useFocusReturnAfterClose";
import { focusFellToPage } from "../components/focus/regionFocus";
import { useHasPainted } from "../components/motion/useHasPainted";
import { StatusBadge } from "../components/status/StatusBadge";
import { StatusMark } from "../components/status/StatusMark";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { STATUS_TONE, StatusGlyph, type StatusEntry } from "../configuration/importParts";
import type { Translate } from "../i18n/catalog";
import { InlinePath } from "../i18n/InlinePath";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import type { RemoteDeviceKeyRemoval, ShellSystemChange, ShellSystemChangeResult, ShellSystemChangesFailure, ShellSystemChangesListed } from "../platform";
import { commandLineError } from "../platform/commandLineError";
import { ringForAutomaticFocus } from "../input/lastInput";
import { usePlatform } from "../platform/react";
import { Button } from "../primitives";
import type { ProviderInfo } from "../protocol";
import { DeviceKeyFileIncomplete, DeviceKeyLineLeft, DevicePrivateKeyLeft } from "../remoteDaemons/deviceKeyRemoval";
import { useDaemonStore, useProviders } from "../store";
import { commandLineFailureWords } from "./CommandLineSetting";
import { COMMAND_LINE_TOOL } from "./commandLineTool";
import { SettingRow } from "./SettingRow";
import { SettingsFocusContext, useSectionRefocus } from "./useSectionRefocus";

/** What is in effect as the row and the modal read it: the items of the parts that answered, and what could not be
 * read as the raw texts behind its alert's Details, the application's (the shell itself where it did not answer, and
 * each of its features that could not tell) apart from the daemon's. */
interface InEffect {
  changes: ShellSystemChange[];
  appFailed: string[];
  daemonFailed: string[];
}

/** What the last cleanup came to: what became of each item, and what failed as a whole, as the raw texts behind its
 * alert's Details. */
interface Outcome {
  results: ShellSystemChangeResult[];
  appFailed: string[];
  daemonFailed: string[];
}

/** The kind a failure carries for this computer's daemon not answering. */
const DAEMON = "daemon";

/** How long focus waits, after the modal closes, for a read of the row still out to settle before it chooses between
 * the row's action and the section's entry. */
const READ_SETTLE_MS = 3000;

/** The command that uninstalls a daemon that runs on its own, on its server; it cleans up as its first step. */
const UNINSTALL_COMMAND = `${COMMAND_LINE_TOOL} uninstall`;

/**
 * The cleanup before the application is uninstalled, as one row of Settings' Cleanup section: what it is for, whether
 * anything is in effect (a count, never a list), and the one action that clears all of it and turns those features off
 * (P9 in `docs/conventions/principles.md`). Nothing here clears one item: one is undone at its own feature's switch,
 * which the row says once.
 *
 * The action is offered only on the computer itself, in the local daemon's window of the desktop application, which
 * has the one end that is asked: the shell (`systemChanges`), answering the application's own items and this
 * computer's daemon's in one list. Anywhere else the row is a sentence and has no button, and nothing is listed, read
 * or cleared from there: a remote daemon's window of the desktop application; a browser on a desktop application's own
 * daemon; and a browser on a daemon that runs on its own (the server form), where the sentence says that uninstalling
 * it cleans up first, on the server itself, and the command that uninstalls it stands under the row to copy, never run. A browser tells the two
 * daemons apart by what the daemon says of itself in its snapshot (`host.application`), and shows nothing until it has.
 *
 * The action opens a warning modal: the shell is asked again as it opens, Confirm held meanwhile, and it then says in
 * one sentence what the cleanup does, counts what will be removed (the count opens their list, folded away to begin
 * with), says in one line what is kept, and has the field the action's word is typed into before Confirm is enabled.
 * A feature of the application that cannot tell what it has, or the daemon not answering, is an alert above the count
 * of the rest, and Confirm is offered as long as something is listed: confirming also clears what could not be read,
 * which each alert then says. One Try again, on the last alert, reads everything again. Confirming clears everything,
 * and the result takes the modal's place: whether everything went, each item that did not with what stays and where,
 * an alert for what failed as a whole, the count of what was removed (opening their list), and one button to close.
 * Focus goes to the result's first line, what happened, so it is read with the dialog's title. The row is read again,
 * and shows the new state.
 *
 * However the modal closes, focus comes back to the row's action, or to the section's entry once nothing is left and
 * the action is disabled: the result's own return target is the confirmation's button, which is gone by then. Where
 * the row's read is still out as the modal closes (the one after a clearing, say), the choice waits for it to settle,
 * for at most `READ_SETTLE_MS`, so it is made on what is left and not on what was there before.
 */
export function UninstallCleanup(): React.ReactElement | null {
  const t = useT();
  const platform = usePlatform();
  const shell = platform.systemChanges;
  const host = useDaemonStore((s) => s.host);
  const applicationDaemon = host?.application === true;
  const [inEffect, setInEffect] = useState<InEffect>();
  const [dialog, setDialog] = useState<"reading" | "confirm" | Outcome>();
  // How many times Try again has been answered in the open confirmation: a failure it leaves is announced anew.
  const [attempt, setAttempt] = useState(0);
  const asked = useRef(0);
  // The row's read still out, if any; and whether the row is still mounted, for a focus return that waits on it.
  const reading = useRef<Promise<unknown>>(undefined);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  // Counts the modal's openings and closings: a read for the modal settles it only while the modal it was made for is
  // still the one on screen, so a read still out when the modal is cancelled never opens it again.
  const shown = useRef(0);
  const actionRef = useRef<HTMLButtonElement>(null);
  const sectionEntry = useContext(SettingsFocusContext);
  const returnFocus = useFocusReturnAfterClose(() => {
    const choose = () => {
      const action = actionRef.current;
      (action && !action.disabled ? action : sectionEntry())?.focus();
    };
    const out = reading.current;
    if (!out) return choose();
    // Chosen once the read has settled and the row has rendered what it found, unless focus has gone elsewhere since.
    const waited = new Promise((settle) => setTimeout(settle, READ_SETTLE_MS));
    void Promise.race([out.catch(() => undefined), waited]).then(() =>
      afterFrames(1, () => {
        if (!mounted.current || !focusFellToPage()) return;
        ringForAutomaticFocus();
        choose();
      }),
    );
  });

  const read = useCallback(async (): Promise<InEffect | undefined> => {
    if (!shell) return undefined;
    asked.current += 1;
    const moment = asked.current;
    const listed = shell.list().then(inEffectOf, (cause: unknown) => ({ changes: [], ...failedTexts([], cause) }));
    reading.current = listed;
    const answer = await listed;
    if (reading.current === listed) reading.current = undefined;
    if (moment !== asked.current) return undefined;
    setInEffect(answer);
    return answer;
  }, [shell]);

  useEffect(() => {
    void read();
    return () => {
      asked.current += 1;
    };
  }, [read]);

  const unread = inEffect !== undefined && inEffect.appFailed.length + inEffect.daemonFailed.length > 0;
  const count = inEffect?.changes.length ?? 0;
  const nothing = inEffect !== undefined && !unread && count === 0;
  // The button is disabled once nothing is left, which can be while focus is back on it from the result.
  useSectionRefocus([nothing]);

  const label = t("settings.uninstall.row.label");
  if (!shell) {
    if (platform.kind !== "browser") return <SettingRow searchId="cleanup.uninstall" label={label} description={<Message id="settings.uninstall.elsewhere.remote" />} />;
    // A browser cannot tell which daemon it is on until the daemon has said (`host.application`).
    if (!host) return null;
    if (applicationDaemon) return <SettingRow searchId="cleanup.uninstall" label={label} description={<Message id="settings.uninstall.elsewhere" />} />;
    return (
      <SettingRow
        searchId="cleanup.uninstall"
        label={label}
        description={<Message id="settings.uninstall.elsewhere.server" />}
        below={<CopyCommand text={UNINSTALL_COMMAND} copyText={t("settings.uninstall.elsewhere.server.copy")} failedText={t("settings.uninstall.elsewhere.server.copyFailed")} />}
      />
    );
  }

  const close = () => {
    shown.current += 1;
    setDialog(undefined);
    returnFocus();
  };

  /** Asks the shell again for the modal: it stays open on the answer, unless all of it answered and nothing is left.
   * `retry` is Try again's read, where the one made as the modal opens is not. */
  const readForDialog = async (retry = false) => {
    // The modal this read is for: opening it is a new one, Try again keeps it.
    if (!retry) shown.current += 1;
    const showing = shown.current;
    setDialog("reading");
    const fresh = await read();
    // Superseded by a later read, which settles the modal, or the modal was closed meanwhile.
    if (!fresh || showing !== shown.current) return;
    if (retry) setAttempt((answered) => answered + 1);
    if (fresh.changes.length === 0 && fresh.appFailed.length + fresh.daemonFailed.length === 0) close();
    else setDialog("confirm");
  };

  const clear = async () => {
    const outcome: Outcome = await shell.clearAll().then(
      ({ results, failed }) => ({ results, ...failedTexts(failed) }),
      (cause: unknown) => ({ results: [], ...failedTexts([], cause) }),
    );
    setDialog(outcome);
    await read();
  };

  const status = inEffect === undefined ? undefined : unread ? t("settings.uninstall.row.unread") : count === 0 ? t("settings.uninstall.row.nothing") : t("settings.uninstall.row.count", { count });
  return (
    <>
      <SettingRow compact searchId="cleanup.uninstall" label={label} status={status} description={<Message id="settings.uninstall.row.description" />}>
        {/* Nothing to remove is what the row says, so the button gives no reason of its own. */}
        <Button ref={actionRef} size="sm" variant="danger" focusOnPress={false} disabled={inEffect === undefined || nothing} onPress={() => void readForDialog()}>
          {t("settings.uninstall.clear.action")}
        </Button>
      </SettingRow>
      {(dialog === "reading" || dialog === "confirm") && inEffect && (
        <ConfirmDialog
          title={t("settings.uninstall.confirm.title")}
          errorTitle={t("settings.uninstall.confirm.failedTitle")}
          subject={<Message id="settings.uninstall.confirm.summary" />}
          message={<ConfirmBody inEffect={inEffect} reading={dialog === "reading"} attempt={attempt} onTryAgain={() => void readForDialog(true)} />}
          confirmLabel={t("settings.uninstall.clear.action")}
          destructive
          typeToConfirm={t("dialog.typeToConfirm.remove")}
          waiting={dialog === "reading"}
          confirmDisabled={count === 0}
          size="lg"
          onConfirm={clear}
          onCancel={close}
        />
      )}
      {typeof dialog === "object" && (
        <Dialog title={t("settings.uninstall.result.title")} focusesItselfOnOpen onClose={close} size="lg" footer={<Button type="button" variant="secondary" onPress={close}>{t("common.close")}</Button>}>
          <Results outcome={dialog} />
        </Dialog>
      )}
    </>
  );
}

function inEffectOf({ changes, failed }: ShellSystemChangesListed): InEffect {
  return { changes, ...failedTexts(failed) };
}

/** What failed as a whole, as the raw texts behind its alert's Details: the application's (the shell itself where it
 * did not answer, `unanswered`, and each of its features that could not tell) apart from the daemon's. Both empty when
 * all of it answered. */
function failedTexts(failed: ShellSystemChangesFailure[], unanswered?: unknown): { appFailed: string[]; daemonFailed: string[] } {
  return {
    appFailed: [...(unanswered === undefined ? [] : [unanswered instanceof Error ? unanswered.message : String(unanswered)]), ...failed.filter(({ kind }) => kind !== DAEMON).map(({ kind, detail }) => `${kind}: ${detail}`)],
    daemonFailed: failed.filter(({ kind }) => kind === DAEMON).map(({ detail }) => detail),
  };
}

function lineKey(change: ShellSystemChange): string {
  return change.kind === "device_key" ? `device_key:${change.id}` : change.kind === "account_sharing" ? `account_sharing:${change.account}` : change.kind;
}

/** An item as it is listed: what it is; the one fact that tells it from the others of its kind (a value, never a
 * sentence), in its own direction, with `factText` the same fact as plain text for the tooltip a clipped one carries;
 * `named`, a name the user gave that tells it apart where the fact does not, which is kept whole while the fact is
 * clipped; and, for the two cases that need a word more, a qualifier. A path is `clipped` at its start, so that what it
 * ends with stays readable. `note` is what removing the item brings with it that its line cannot say, which the
 * confirmation's list carries behind a mark at the line's end. */
interface Line {
  key: string;
  name: string;
  fact?: React.ReactNode;
  factText?: string;
  named?: string;
  monospace?: boolean;
  clipped?: "start";
  qualifier?: string;
  note?: string;
}

/** Whom a device key signs in as, the fact of its line. */
function signsInAs(change: Extract<ShellSystemChange, { kind: "device_key" }>): string {
  return `${change.user}@${change.host}`;
}

/** The items as their lines, in the order given. Two device keys that sign in as the same user on the same host (two
 * saved remote daemons that differ by port) also carry the names of their saved daemons, which tell them apart. */
function linesOf(t: Translate, providers: readonly ProviderInfo[], changes: readonly ShellSystemChange[]): Line[] {
  const addresses = changes.flatMap((change) => (change.kind === "device_key" ? [signsInAs(change)] : []));
  const repeated = new Set(addresses.filter((address, index) => addresses.indexOf(address) !== index));
  return changes.map((change) => lineOf(t, providers, change, repeated));
}

/** An item as its line: a device key by the user and host it signs in as (with a qualifier where only its private half
 * is left on this computer, and its saved daemon's name where the address is one of `repeated`), the login entry by
 * nothing more (with a qualifier where it is turned off in the system's list and still there), the command-line tool by
 * its link's path, an account that shares by its agent and its name. */
function lineOf(t: Translate, providers: readonly ProviderInfo[], change: ShellSystemChange, repeated: ReadonlySet<string>): Line {
  const key = lineKey(change);
  switch (change.kind) {
    case "device_key": {
      const fact = signsInAs(change);
      return {
        key,
        name: t("settings.uninstall.item.deviceKey.label"),
        fact,
        factText: fact,
        monospace: true,
        named: repeated.has(fact) ? change.name : undefined,
        qualifier: change.onServer ? undefined : t("settings.uninstall.item.deviceKey.privateOnly"),
      };
    }
    case "launch_at_login":
      return { key, name: t("settings.launchAtLogin.label"), qualifier: change.turnedOff ? t("settings.uninstall.item.launchAtLogin.turnedOff") : undefined };
    case "command_line":
      return { key, name: t("settings.commandLine.label"), fact: change.path, factText: change.path, monospace: true, clipped: "start" };
    case "account_sharing": {
      const agent = providerLabel(t, providers, change.agent);
      return {
        key,
        name: t("settings.uninstall.item.sharing.label"),
        // Two values side by side, told apart by the gap and not by a word a language would have to order: each is
        // isolated and keeps its own direction, and the two follow the interface's.
        fact: <bdi>{agent}</bdi>,
        factText: agent,
        named: change.name,
        note: t("settings.uninstall.confirm.sessions"),
      };
    }
  }
}

/** The one line an item is, wherever it is listed: what it is in the text's colour, then its fact in the secondary
 * colour (a path or an address in the monospace face and left to right), then the name that tells it apart, then its
 * qualifier. It never wraps: a long fact is clipped with a fade and carries its full text as a tooltip while it is, and
 * the qualifier stays whole after it. The name gives way only once the fact has nothing left to give (the fact shrinks
 * first, by its far greater shrink factor), since two items of one fact differ by it alone, and is capped so that a
 * long one leaves the fact some room. As the `title` of a block of its own, what it is takes the weight of a row's
 * name. With `noted`, an item that has a note ends in the mark that opens it, which stays whole as the qualifier does:
 * it is the fact that is clipped. */
function ItemLine({ line, title = false, noted = false }: { line: Line; title?: boolean; noted?: boolean }): React.ReactElement {
  return (
    <div className="flex min-w-0 items-baseline gap-2">
      <span className={`shrink-0 text-foreground ${title ? "font-medium" : ""}`}>{line.name}</span>
      {line.fact !== undefined && (
        <FadeOverflow as="span" dir={line.monospace ? "ltr" : undefined} clip={line.clipped} titleWhenClipped={line.factText} className={`min-w-0 text-app-muted ${line.named !== undefined ? "shrink-[999]" : ""} ${line.monospace ? "font-mono text-xs leading-5" : ""}`}>
          {line.fact}
        </FadeOverflow>
      )}
      {line.named !== undefined && (
        <FadeOverflow as="span" titleWhenClipped={line.named} className="min-w-0 max-w-[60%] text-app-muted">
          <bdi>{line.named}</bdi>
        </FadeOverflow>
      )}
      {line.qualifier !== undefined && <span className="shrink-0"><StatusBadge>{line.qualifier}</StatusBadge></span>}
      {noted && line.note !== undefined && <span className="shrink-0 self-center"><StatusMark inline tone="note" label={line.note} /></span>}
    </div>
  );
}

/** The glyph that leads a line of the result, with the word for it for a screen reader, which hears each line's
 * outcome before the line. */
function OutcomeGlyph({ entry }: { entry: StatusEntry }): React.ReactElement {
  const t = useT();
  return (
    <span className="flex">
      <StatusGlyph entry={entry} size="mt-0.5 size-4" />
      <span className="sr-only">{t(entry.label)}</span>
    </span>
  );
}

/** The list a disclosure folds away: one line per item, every line of one shape. In the result each line is led by
 * the glyph of its `outcome`. Its glyphs and texts stand under those of the blocks above it only because
 * `TextDisclosure` sets what it holds in by its triangle and the gap after it, which together match a card's padding:
 * nothing here sets the list in, so a change to either has to keep the two equal. */
function ItemList({ lines, outcome, noted }: { lines: Line[]; outcome?: StatusEntry; noted?: boolean }): React.ReactElement {
  return (
    <ul className="flex min-w-0 flex-col gap-1">
      {lines.map((line) => (
        <li key={line.key} className="flex min-w-0 items-start gap-2">
          {outcome && <OutcomeGlyph entry={outcome} />}
          <div className="min-w-0 flex-1"><ItemLine line={line} noted={noted} /></div>
        </li>
      ))}
    </ul>
  );
}

/** A count that opens the list of the items it counts, folded away to begin with: content shown for reading, so the
 * disclosure is a `TextDisclosure`, the count after its triangle. What it holds is set in under the count, so that what
 * follows the fold in the dialog is seen to stand outside it. `triggerId` is the count's, for a caller that hands focus
 * to it. */
function FoldedItems({ summary, lines, outcome, noted, triggerId }: { summary: string; lines: Line[]; outcome?: StatusEntry; noted?: boolean; triggerId?: string }): React.ReactElement {
  return (
    <TextDisclosure id={triggerId} label={summary}>
      <ItemList lines={lines} outcome={outcome} noted={noted} />
    </TextDisclosure>
  );
}

/** Marks the one Try again of the confirmation, for the handoff of its focus. */
const RETRY = "data-cleanup-retry";

/** What the confirmation says under its one sentence: an alert for each part that could not be read (the
 * application's features, and the daemon); the count of what will be removed, which opens their list (an account that
 * shares ending in the mark that says what happens to its idle sessions); and one line on what is kept (the app's own
 * data and, when an account shares or may, the history sharing moved).
 *
 * Confirming clears a part that could not be read too, so while something is listed and Confirm is offered, each alert
 * says so; and with the daemon unread, whose accounts may share, the line on what is kept speaks of sharing's history
 * and one sentence says what happens to idle sessions, which no line is there to carry a mark for.
 *
 * Both alerts are recovered from by the same read, so there is one Try again, on the last of them, `pending` while
 * `reading`. An alert that is there as the dialog opens is part of what the dialog opened with and is not announced;
 * one that arrives later is, after the delay a focus move needs, and again for each `attempt` that leaves it, once
 * each: the warning modal is an alert dialog, which would also speak the alert's text as it is put in place. When Try
 * again goes with its alert while it holds focus, focus moves to the Try again that took its place on the other alert,
 * else to the count, which is what the read changed, else to the dialog. */
function ConfirmBody({ inEffect, reading, attempt, onTryAgain }: { inEffect: InEffect; reading: boolean; attempt: number; onTryAgain: () => void }): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  const painted = useHasPainted();
  const keptRef = useRef<HTMLParagraphElement>(null);
  const countId = useId();
  const handoff = useHandoffAfterRemoval(() => dialogAround(keptRef)?.querySelector<HTMLElement>(`[${RETRY}] button`) ?? document.getElementById(countId) ?? dialogAround(keptRef));
  const lines = linesOf(t, providers, inEffect.changes);
  const appUnread = inEffect.appFailed.length > 0;
  const daemonUnread = inEffect.daemonFailed.length > 0;
  const offered = lines.length > 0;
  const sharing = inEffect.changes.some(({ kind }) => kind === "account_sharing");
  const tryAgain = (
    <>
      {handoff(`[${RETRY}]`)}
      <span {...{ [RETRY]: "" }}><Button size="sm" variant="secondary" pending={reading} onPress={onTryAgain}>{t("error.tryAgain")}</Button></span>
    </>
  );
  const stillCleared = offered && <Message id="settings.uninstall.readFailed.body" params={{ action: <InlineReference>{t("settings.uninstall.clear.action")}</InlineReference> }} />;
  const announcement = { announce: painted, announceKey: attempt, announceDelayMs: AFTER_FOCUS_MOVE_DELAY_MS, delayEachAttempt: true, insideAlertDialog: true };
  return (
    <>
      {appUnread && (
        <ErrorAlert {...announcement} title={t("settings.uninstall.readFailed.app.title")} details={<DiagnosticBlock wrap>{inEffect.appFailed.join("\n")}</DiagnosticBlock>} actions={daemonUnread ? undefined : tryAgain}>
          {stillCleared}
        </ErrorAlert>
      )}
      {daemonUnread && (
        <ErrorAlert {...announcement} title={t("settings.uninstall.readFailed.daemon.title")} details={<DiagnosticBlock wrap>{inEffect.daemonFailed.join("\n")}</DiagnosticBlock>} actions={tryAgain}>
          {stillCleared}
        </ErrorAlert>
      )}
      {offered && <FoldedItems summary={t("settings.uninstall.confirm.items", { count: lines.length })} lines={lines} noted triggerId={countId} />}
      {daemonUnread && offered && <p className="text-app-muted"><Message id="settings.uninstall.confirm.sessions.unread" /></p>}
      <p ref={keptRef} className="text-app-muted"><Message id={sharing || (daemonUnread && offered) ? "settings.uninstall.confirm.kept.sharing" : "settings.uninstall.confirm.kept"} /></p>
    </>
  );
}

const ALL_REMOVED: StatusEntry = { label: "settings.uninstall.done.all", icon: CircleCheck, tone: STATUS_TONE.success };
const NOT_KNOWN: StatusEntry = { label: "settings.uninstall.done.unknown", icon: TriangleAlert, tone: STATUS_TONE.warning };
const REMOVED: StatusEntry = { label: "settings.uninstall.result.cleared", icon: CircleCheck, tone: STATUS_TONE.success };
const NOT_REMOVED: StatusEntry = { label: "settings.uninstall.result.left", icon: CircleAlert, tone: STATUS_TONE.danger };

/** Why an item was not removed, as a report of outcomes writes it (`docs/conventions/ui-standard.md`, "Alerts and
 * marks"): `reason`, one short plain sentence, and `remedy`, what to do about it or where to remove what stays, both
 * written under the item's line; and `details`, the rest (what stays of a device key in full, the system's own words),
 * which its Details disclosure holds (none where there is nothing more to say). */
interface Left {
  reason: React.ReactNode;
  remedy?: React.ReactNode;
  details?: React.ReactNode;
}

/** What the last cleanup came to, as a report of outcomes. It opens with one line, under its status glyph, saying
 * whether everything went or how many items did and did not, and, where no listed item is known to be left but a part
 * failed as a whole, that it cannot be told whether everything went. It mounts in the commit that unmounts the
 * confirmation, whose focus goes with it and which react-aria would hand back to nothing, so it puts focus on that line
 * itself (`useFocusWhenShown`, the dialog passing `focusesItselfOnOpen`), a focusable element that is not a tab stop:
 * focus moving into the dialog onto it reads the dialog's title and then the line, in one phrase. The move follows the
 * last input's ring. It waits until the window is attended, as it is not after a system prompt the clearing raised (an
 * administrator's password, cancelled or not) has taken focus, and is made again each time the window becomes attended
 * while the result is open, passing through a hidden relay so the line takes focus anew: coming back to the window,
 * VoiceOver otherwise speaks the window's own controls rather than what is focused in it. A part
 * that failed as a whole (the application did not answer, one of its features could not tell what it has, or the daemon
 * did not answer) is an Alert next, whose items are not among the lines and may or may not have been removed: the Alert
 * says so, and what failed is behind its Details. Then the items that were not removed, each a block of its own with
 * the outline of a flat card (`FlatCard`'s, whose section and heading a list item cannot take): the failure glyph, the
 * item's line as the block's title, and hanging under the title's text the reason and the remedy (`Left`), then a
 * Details disclosure with the rest. The items that were removed are counted last, their list folded away as plain lines, each
 * led by the success glyph in the column of the blocks' glyphs. */
function Results({ outcome }: { outcome: Outcome }): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  const { host } = usePlatform();
  const failureNode = useFailureNode();
  const summaryRef = useRef<HTMLParagraphElement>(null);
  const relayRef = useRef<HTMLSpanElement>(null);
  const attended = useWindowAttended();
  useFocusWhenShown(
    () => {
      ringForAutomaticFocus();
      return summaryRef.current;
    },
    attended,
    undefined,
    relayRef,
  );
  const lines = linesOf(t, providers, outcome.results.map(({ change }) => change));
  const items = outcome.results.map((result, index) => ({ line: lines[index], left: leftOf(t, result, host.os === "macos", failureNode) }));
  const left = items.filter((item): item is { line: Line; left: Left } => item.left !== undefined);
  const removed = items.filter((item) => item.left === undefined).map((item) => item.line);
  const complete = left.length === 0 && outcome.appFailed.length + outcome.daemonFailed.length === 0;
  const status = complete ? ALL_REMOVED : NOT_KNOWN;
  const summary = left.length === 0 ? t(status.label) : removed.length === 0 ? t("settings.uninstall.done.left", { count: left.length }) : t("settings.uninstall.done.mixed", { removed: removed.length, count: left.length });
  return (
    <div className="flex flex-col gap-3 text-sm">
      <span ref={relayRef} tabIndex={-1} aria-hidden="true" className="absolute" />
      <p ref={summaryRef} tabIndex={-1} className="flex items-start gap-1.5 rounded-sm outline-none ui-keyboard-focus:ring-2 ui-keyboard-focus:ring-focus">
        <StatusGlyph entry={status} size="mt-0.5 size-4" />
        <span>{summary}</span>
      </p>
      {outcome.appFailed.length > 0 && (
        <ErrorAlert title={t("settings.uninstall.result.appFailed.title")} details={<DiagnosticBlock wrap>{outcome.appFailed.join("\n")}</DiagnosticBlock>}>{t("settings.uninstall.result.failed.body")}</ErrorAlert>
      )}
      {outcome.daemonFailed.length > 0 && (
        <ErrorAlert title={t("settings.uninstall.result.daemonFailed.title")} details={<DiagnosticBlock wrap>{outcome.daemonFailed.join("\n")}</DiagnosticBlock>}>{t("settings.uninstall.result.failed.body")}</ErrorAlert>
      )}
      {left.length > 0 && (
        <ul className="flex min-w-0 flex-col gap-2">
          {left.map(({ line, left: { reason, remedy, details } }) => (
            <li key={line.key} className="grid min-w-0 grid-cols-[auto_minmax(0,1fr)] gap-x-2 gap-y-1.5 rounded-xl panel-edge px-4 py-3">
              <OutcomeGlyph entry={NOT_REMOVED} />
              <ItemLine line={line} title />
              <p className="col-start-2">{reason}</p>
              {remedy && <p className="col-start-2">{remedy}</p>}
              {details !== undefined && <div className="col-start-2 min-w-0"><DetailsDisclosure label={t("error.details")} indented>{details}</DetailsDisclosure></div>}
            </li>
          ))}
        </ul>
      )}
      {removed.length > 0 && <FoldedItems summary={t("settings.uninstall.result.removed", { count: removed.length })} lines={removed} outcome={REMOVED} />}
    </div>
  );
}

/** One part of what a Details disclosure holds: what stays and where, in the application's words, and under it the
 * system's own. */
function Stays({ children, detail }: { children: React.ReactNode; detail?: string }): React.ReactElement {
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <p>{children}</p>
      {detail && <DiagnosticBlock wrap>{detail}</DiagnosticBlock>}
    </div>
  );
}

/** Why an item was not removed and what stays of it; nothing of one that was removed. */
function leftOf(t: Translate, { outcome }: ShellSystemChangeResult, macos: boolean, failureNode: ReturnType<typeof useFailureNode>): Left | undefined {
  switch (outcome.result) {
    case "cleared":
      return undefined;
    case "device_key_left": {
      const { line, privateKeyLeft } = outcome.removal;
      // Each part that stays has its sentence: the line on the server, or its file, and the private key here.
      const reasons = [
        ...(line.result === "left_on_host" ? [t("settings.uninstall.result.reason.keyLine")] : []),
        ...(line.result === "file_incomplete" ? [t("settings.uninstall.result.reason.fileIncomplete")] : []),
        ...(privateKeyLeft !== null ? [t("settings.uninstall.result.reason.privateKey")] : []),
      ];
      return { reason: reasons.join(t("common.sentenceSeparator")), details: <DeviceKeyLeft removal={outcome.removal} /> };
    }
    case "launch_at_login_left":
      // Its sentence says why and where to remove it, both under the line.
      return { reason: <LoginEntryLeft macos={macos} />, details: systemWords(outcome.detail) };
    case "command_line_left": {
      const error = commandLineError(outcome.error);
      // The setting's own sentence says why, and what to do where there is something; where it could only say that the
      // change could not be made, the reason already does. A foreign link's detail is the target, which the sentence says.
      const named = error.path !== null && (error.code === "refused" || error.code === "not_a_link" || (error.code === "foreign_link" && Boolean(error.detail)));
      return { reason: t("settings.uninstall.result.reason.commandLine"), remedy: named ? commandLineFailureWords(error) : undefined, details: error.code === "foreign_link" ? undefined : systemWords(error.detail) };
    }
    case "account_sharing_left":
      return { reason: failureNode(outcome.failure, "summary"), remedy: failureNode(outcome.failure, "remedy") || undefined, details: failureNode(outcome.failure, "details") || undefined };
  }
}

/** The system's own words behind a Details disclosure, or no Details where it gave none. */
function systemWords(detail: string | null | undefined): React.ReactNode {
  return detail ? <DiagnosticBlock wrap>{detail}</DiagnosticBlock> : undefined;
}

/** That the system kept the login entry, and where the user removes it, as one path: System Settings' Login Items on
 * macOS, Task Manager's Startup apps on Windows. */
function LoginEntryLeft({ macos }: { macos: boolean }): React.ReactElement {
  const t = useT();
  if (!macos) return <Message id="settings.uninstall.result.launchAtLogin.left.windows" params={{ path: <InlinePath steps={[t("settings.launchAtLogin.step.taskManager"), t("settings.launchAtLogin.step.startupApps")]} /> }} />;
  return <Message id="settings.uninstall.result.launchAtLogin.left" params={{ path: <InlinePath steps={[t("settings.launchAtLogin.step.general"), t("settings.launchAtLogin.step.loginItems")]} /> }} />;
}

/** The two parts of a device key that was not wholly taken back, each as the Remote devices page words it
 * (`deviceKeyRemoval.tsx`): its line on the server, named with its mark and its file so the owner can look for it
 * there, or that the server's file may be incomplete and where its earlier content is kept; and its private key in
 * this computer's credential store. Where the line did go and the private key did not, it says the line went. */
function DeviceKeyLeft({ removal }: { removal: RemoteDeviceKeyRemoval }): React.ReactElement {
  const { line, privateKeyLeft } = removal;
  return (
    <>
      {line.result === "left_on_host" && <Stays detail={line.detail}><DeviceKeyLineLeft line={line} privateKeyRemoved={privateKeyLeft === null} /></Stays>}
      {line.result === "file_incomplete" && <Stays detail={line.detail}><DeviceKeyFileIncomplete line={line} /></Stays>}
      {line.result === "removed" && <p><Message id="settings.uninstall.result.deviceKey.lineRemoved" /></p>}
      {privateKeyLeft !== null && <Stays detail={privateKeyLeft.detail}><DevicePrivateKeyLeft left={privateKeyLeft} /></Stays>}
    </>
  );
}
