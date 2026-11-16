import React, { useEffect, useRef, useState } from "react";

import { AlertButton } from "../components/alerts/AlertButton";
import { FailureAlert } from "../components/alerts/FailureAlert";
import { ChangeAnnouncer } from "../components/announce/ChangeAnnouncer";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { errorPresentation, type ErrorPresentation } from "../errors/errorPresentation";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { useLatest } from "../components/hooks/useLatest";
import { ActionableError } from "../errors/actionableError";
import { InlineReference } from "../i18n/InlineReference";
import { Dialog } from "../components/dialog/Dialog";
import { useDialogAction, useRefocusIfLost, useSubmitValidation } from "../components/dialog/dialogHooks";
import { dialogAround } from "../components/dialog/dialogElement";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { replaceOverrides, shortcutOverrides, type Overrides } from "../shortcuts/store";
import { writeConflict } from "../errors/writeConflict";
import { useDaemon, useDaemonStore, useProviders } from "../store";
import { languageSettingValues } from "./languageSettingValues";
import { applyClientSettings, clientSettingsOf, clientSettingValues, type ClientSettings } from "./clientSettings";
import { sessionOrderValues } from "./importValues";
import { useClientSettingsReview } from "./useClientSettingsReview";
import { useImportChoices } from "./useImportChoices";
import { useHostPaths } from "../components/hooks/useHostPaths";
import { onWindowFocusReturn } from "../components/hooks/windowFocus";
import { fileFailure } from "./failureText";
import {
  accountResolutionsOf,
  carriedParts,
  documentShortcuts,
  hasAccountDirectoryProblem,
  hasDirectoryProblem,
  importedAgents,
  initialAccountReview,
  initialReview,
  resolutionsOf,
  reviewForOffers,
  shortcutsRecord,
  shortcutsReview,
  accountReviewForInspection,
  takesDocumentCloneDir,
  withCloneDirOffers,
  type AccountReviewState,
  nothingToImport,
  type Inspection,
  type ReviewState,
} from "./importPlan";
import { ImportProgressView } from "./ImportProgress";
import { newImportId, NO_PROGRESS, withProgress, type ImportProgress } from "./progressState";
import { ImportResult, type Imported } from "./ImportResult";
import { ImportReview } from "./ImportReview";
import type { Region } from "./RegionTabs";
import { raiseError } from "../components/alerts/raiseError";
import { ringForAutomaticFocus } from "../input/lastInput";
import { Button } from "../primitives";
import { TitledControl } from "../components/hint/TitledControl";

type Stage =
  | { kind: "choose" }
  | { kind: "review"; fileName: string; document: unknown; inspection: Inspection }
  | { kind: "importing"; inspection: Inspection }
  | { kind: "result"; result: Imported; shortcuts?: { applied: number; skipped: number; overrides: Overrides }; clientSettings: ClientSettings };

/** How long after the window regains focus a file may still arrive before the picker is taken as
 * dismissed: a browser reports the chosen file just after the focus returns. */
export const FOCUS_GRACE_MS = 500;

/** Runs the platform's file picker: what it returns, or `undefined` once `failed` has been told why it threw; `settled`
 * runs either way. A function of its own because the React Compiler skips whatever holds a `try` with a `finally`. */
async function settleOpen<T>(open: () => Promise<T>, failed: (err: unknown) => void, settled: () => void): Promise<T | undefined> {
  try {
    return await open();
  } catch (err) {
    failed(err);
    return undefined;
  } finally {
    settled();
  }
}

/** Runs `work` with `subscribe`'s subscription held for as long as it takes. A function of its
 * own because the React Compiler skips whatever holds a `try` with a `finally`, as `settleOpen` explains. */
async function whileListening<T>(subscribe: () => () => void, work: () => Promise<T>): Promise<T> {
  const unsubscribe = subscribe();
  try {
    return await work();
  } finally {
    unsubscribe();
  }
}

/**
 * Imports a configuration document in three steps inside one dialog. The user picks a file through the platform's open
 * dialog and the daemon inspects it; what it answers is reviewed here, a row for every project and for every account,
 * before anything is created; and applying sends the document as it was read with the choice made for each project and
 * account, and shows what came of it: while the daemon works, the body lists every item being imported with the state
 * the daemon's progress reports give it (`ImportProgressView`), and the result takes its place when the reply comes.
 * The parts of the file are imported as the user checks them (the consoles and projects and the accounts by default,
 * the Settings not, nor accounts that bring nothing for any agent, only the three global switches; once the Settings
 * are checked, each section the file carries is imported unless unchecked in the Settings tab), and only a checked part
 * is sent, reviewed or shown in the result; with nothing chosen (no part, or only Settings with every section
 * unchecked) there is nothing to import. A directory typed for a project or an account that the daemon would refuse
 * stops the import at the row, bringing its tab forward. A document the daemon refuses leaves the dialog on the first
 * step, with the reason, for another file to be tried. The settings only this client holds (appearance, Markdown style,
 * terminal font size, running in the background, the menu bar icon, session order) and the keyboard shortcuts travel
 * in the settings of their sections: the review says what each would change here, and they are applied here from the
 * daemon's reply, for what it says it applied. The language is the daemon's own: the review lists it with the
 * General settings and the daemon applies it, after which every window follows it. The file's default clone directory
 * is applied with the Git settings unless the user keeps this machine's own. The import carries the settings versions
 * the preview read, and a setting another client changed since has the preview read again while the review is shown,
 * keeping the choices made; one that moves between the last refresh and the import refuses it as a conflict, and the
 * dialog is then back to the first step, for the file to be previewed again.
 * The refresh holds the Import button pending, not the dialog busy; one that fails is shown at the top of the review, with
 * its own Try again or, where no retry can help, a button to choose the file again, and Import is unavailable until the
 * review is current, a press of it taking focus to that button.
 * The daemon's reply to an import comes only after its last clone, so the dialog cannot be dismissed while one is out.
 * Waiting for the file is not such a time: a browser without the file input's `cancel` event never answers for a
 * dismissed picker, so the dialog is held busy only from the file onwards. The button shows it is waiting and ignores
 * another press meanwhile, but Escape and the close button still work. Where the picker is never reported dismissed,
 * the window regaining focus with no file chosen frees the step to be tried again; a file that arrives from that picker
 * after all is still honoured, until a newer press opens another one.
 */
export function ImportConfigurationDialog({ onClose }: { onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request, onImportProgress } = useDaemon();
  const providers = useProviders();
  const { style } = useHostPaths();
  const { configurationFile, host } = usePlatform();
  const { busy, run } = useDialogAction(undefined, t("configuration.import.alertTitle"));
  const { shown, attempt } = useSubmitValidation();
  const settingsVersions = useDaemonStore((s) => s.settings.versions);
  const [stage, setStage] = useState<Stage>({ kind: "choose" });
  const [review, setReview] = useState<ReviewState>([]);
  const [accountReview, setAccountReview] = useState<AccountReviewState>(initialAccountReview(undefined));
  // The review's selected tab, kept here so an import stopped by a row's problem can bring the row's tab forward.
  const [tab, setTab] = useState<Region>();
  // Counts the imports a row's problem stopped, so the first such row gets focus once its message
  // is on screen.
  const [stopped, setStopped] = useState(0);
  // The daemon's progress reports of the import running, which the progress view lists.
  const [progress, setProgress] = useState<ImportProgress>(NO_PROGRESS);
  // A picker is open, or its answer is awaited. A ref as well as state: a second press can land
  // before the render that shows the first.
  const [waiting, setWaiting] = useState(false);
  const waitingRef = useRef(false);
  // The picker the answer of which still counts: a newer press replaces one given up on.
  const pickerRef = useRef(0);
  useEffect(() => {
    if (!waiting) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const givenUp = () => {
      waitingRef.current = false;
      setWaiting(false);
    };
    const onFocus = () => {
      clearTimeout(timer);
      timer = setTimeout(givenUp, FOCUS_GRACE_MS);
    };
    const stop = onWindowFocusReturn(onFocus);
    return () => {
      stop();
      clearTimeout(timer);
    };
  }, [waiting]);
  const inReview = stage.kind === "review" ? stage : undefined;
  const choices = useImportChoices(inReview?.inspection, (offers) => setReview(reviewForOffers(review, offers)));
  const clientSettings = useClientSettingsReview(inReview?.document);
  const currentShortcuts = shortcutOverrides.useValue();
  const shortcuts = inReview ? documentShortcuts(inReview.document) : undefined;
  const bodyRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (stopped > 0) bodyRef.current?.querySelector<HTMLElement>("input[aria-invalid=true]")?.focus();
  }, [stopped]);
  // The result renames the dialog, and focus taken to it reads the new name out; focus left on the
  // close button or the body while the import ran would otherwise hear nothing.
  const finished = stage.kind === "result";
  useEffect(() => {
    if (finished) dialogAround(bodyRef)?.focus();
  }, [finished]);

  const choose = async () => {
    if (!configurationFile || busy || waitingRef.current) return;
    const picker = ++pickerRef.current;
    waitingRef.current = true;
    setWaiting(true);
    const opened = await settleOpen(
      () => configurationFile.open(),
      (err) => {
        if (picker === pickerRef.current) raiseError(fileFailure(t, err, "configuration.import.couldNotRead"), { title: t("configuration.import.alertTitle") });
      },
      () => {
        if (picker === pickerRef.current) {
          waitingRef.current = false;
          setWaiting(false);
        }
      },
    );
    if (!opened || picker !== pickerRef.current) return;
    void run(async () => {
      let document: unknown;
      try {
        document = JSON.parse(opened.text);
      } catch {
        errorAlerts.report({
          message: t("configuration.import.notJson", { name: isolate(opened.name) }),
          title: t("configuration.import.alertTitle"),
          recovery: "configuration",
          content: <Message id="configuration.import.notJson" params={{ name: <InlineReference monospace wrap>{opened.name}</InlineReference> }} />,
        });
        return;
      }
      const inspection = await request({ type: "inspect_configuration", document });
      setReview(initialReview(inspection.consoles));
      setAccountReview(initialAccountReview(inspection.accounts));
      choices.reset(inspection);
      setTab(undefined);
      setRefreshFailure(undefined);
      setStage({ kind: "review", fileName: opened.name, document, inspection });
    });
  };

  // A setting the import would write has moved past the version the preview read (another client, an agent or a single
  // action changed it): the preview is read again, with the review's choices kept where they still apply, instead of
  // leaving the import to conflict. A Settings section or the accounts write settings; which fields they would write is
  // for the daemon to say, so a move of any of them counts. A refresh that finds the settings moved again, as one
  // arriving while it ran does, is followed by another.
  const behind =
    stage.kind === "review" &&
    (choices.sections.length > 0 || choices.parts.includes("accounts")) &&
    Object.entries(stage.inspection.settings_versions).some(([path, version]) => (settingsVersions[path] ?? 0) > version);
  const latestStage = useLatest(stage);
  const latestBasis = useLatest({ checked: choices.checked, sectionsChecked: choices.sectionsChecked, useCloneDir: choices.useCloneDir });
  // A refresh is under way, and an import pressed meanwhile would be based on the preview it replaces. A ref as well as
  // state, for the overlap guard to read at once.
  const [refreshing, setRefreshing] = useState(false);
  const refreshingRef = useRef(false);
  // Why the last refresh failed, until a refresh succeeds: it is not tried again by itself, the Alert's Try again does.
  // `attempt` makes a failure that repeats in the same words announce again.
  const [refreshFailure, setRefreshFailure] = useState<{ message: ErrorPresentation; attempt: number }>();
  // Counts the refreshes that replaced the review, for the announcement of a change nothing else marks.
  const [refreshed, setRefreshed] = useState(0);
  // The review is behind and could not be read again: what it shows is not what the import would act on, so Import waits.
  const stuck = behind && !!refreshFailure;
  const refresh = async (reviewing: Extract<Stage, { kind: "review" }>) => {
    if (refreshingRef.current) return;
    refreshingRef.current = true;
    setRefreshing(true);
    const inspection = await request({ type: "inspect_configuration", document: reviewing.document }).catch((err: unknown) => {
      setRefreshFailure((prior) => ({ message: errorPresentation(err), attempt: (prior?.attempt ?? -1) + 1 }));
      return undefined;
    });
    refreshingRef.current = false;
    setRefreshing(false);
    const now = latestStage.current;
    if (!inspection || now.kind !== "review" || now.document !== reviewing.document) return;
    const basis = latestBasis.current;
    setRefreshFailure(undefined);
    setReview((prior) =>
      reviewForOffers(prior, withCloneDirOffers(inspection.consoles, takesDocumentCloneDir(inspection, basis.checked, basis.sectionsChecked, basis.useCloneDir))),
    );
    setAccountReview((prior) => accountReviewForInspection(prior, inspection.accounts));
    setStage({ ...now, inspection });
    setRefreshed((count) => count + 1);
  };
  useEffect(() => {
    if (behind && stage.kind === "review" && !refreshing && !refreshFailure) void refresh(stage);
  }, [behind, refreshing, refreshFailure, stage]);
  // A failure of a review that is no longer behind is stale. Versions only grow, so that is the case only once what made
  // the review behind is unchecked, or a new inspection (the file chosen again) has read them; the next time
  // the review is behind it is read again.
  useEffect(() => {
    if (!behind && refreshFailure) setRefreshFailure(undefined);
  }, [behind, refreshFailure]);
  // The Alert's Try again, or a successful read, takes the Alert away while focus may be on its button. The hand-over is
  // a focus move the user did not make, so the ring follows the last input.
  const importButton = useRef<HTMLButtonElement>(null);
  useRefocusIfLost(() => {
    ringForAutomaticFocus();
    return importButton.current;
  }, [stuck]);
  // The Alert of a failed refresh, whose last button is the one thing to do about it (Try again, or Choose file).
  const refreshAlert = useRef<HTMLDivElement>(null);

  const apply = () => {
    if (stage.kind !== "review" || busy || refreshingRef.current) return;
    if (stuck) {
      // The press of an unavailable Import explains by taking focus to what the Alert offers.
      ringForAutomaticFocus();
      Array.from(refreshAlert.current?.querySelectorAll("button") ?? []).at(-1)?.focus();
      return;
    }
    const accounts = stage.inspection.accounts;
    const { parts, sections } = choices;
    if (nothingToImport(parts, sections)) return;
    const importingConsoles = parts.includes("consoles");
    const creatingAccounts = parts.includes("accounts") && !!accounts;
    const consolesProblem = importingConsoles && hasDirectoryProblem(review, style);
    const accountsProblem = creatingAccounts && hasAccountDirectoryProblem(accounts, accountReview, style);
    if (!attempt(consolesProblem || accountsProblem ? "directory" : undefined)) {
      setTab(consolesProblem ? "consoles" : "accounts");
      setStopped((count) => count + 1);
      return;
    }
    const reviewing = stage;
    const importing: Stage = { kind: "importing", inspection: stage.inspection };
    const importId = newImportId();
    const send = () =>
      request({
        type: "import_configuration",
        import_id: importId,
        document: reviewing.document,
        resolutions: importingConsoles ? resolutionsOf(review) : [],
        apply_settings: sections,
        // The versions the preview read: a setting changed elsewhere since refuses the import as a conflict.
        settings_versions: reviewing.inspection.settings_versions,
        // Absent means true, so only a refusal is spelled out.
        ...(carriedParts(stage.inspection).includes("consoles") && !importingConsoles && { apply_consoles: false }),
        ...(sections.includes("git") && !choices.useCloneDir && { apply_clone_dir: false }),
        ...(creatingAccounts && { apply_accounts: true, apply_agents: importedAgents(providers, accounts, accountReview), account_resolutions: accountResolutionsOf(accounts, accountReview) }),
      });
    void run(async () => {
      setProgress(NO_PROGRESS);
      setStage(importing);
      const result = await whileListening(
        () => onImportProgress(importId, (event) => setProgress((current) => withProgress(current, event))),
        send,
      ).catch((err: unknown) => {
        // The request failed, whether the daemon refused it or the connection closed under it: back to the review,
        // which still holds every choice, and `run` reports the failure. A conflict is the race the review's own
        // refresh could not foresee, the change reaching the daemon before this window: the preview is out of date,
        // and it would conflict again, so it is back to choosing the file, to be previewed again, with the words of an
        // import in place of the generic ones.
        const conflicted = writeConflict(err);
        setStage(conflicted ? { kind: "choose" } : reviewing);
        throw conflicted ? new ActionableError(t("configuration.import.conflict"), "none", { cause: err }) : err;
      });
      // The daemon applied the settings it knows and echoed the ones it passes through, which are this client's own
      // to apply: it does so only for what the daemon says it applied.
      const carried = shortcutsRecord(result.settings_applied?.keyboard_shortcuts);
      const applied = carried && replaceOverrides(carried, host);
      const clientApplied = clientSettingsOf(result.settings_applied, clientSettings.platformSettings);
      applyClientSettings(clientApplied, clientSettings.setAppearance);
      setStage({ kind: "result", result, shortcuts: applied, clientSettings: clientApplied });
    });
  };

  // The footer's buttons stay while the body changes: the same keys in every step's footer keep each one mounted. The
  // import step shows this footer too: the daemon has no way to stop an import, so Cancel is disabled beside the
  // pending Import, and the Import button that was pressed stays the same element and keeps focus as it turns pending.
  const importFooter = (
    <>
      <Button key="cancel" type="button" variant="secondary" onPress={onClose}>
        {t("common.cancel")}
      </Button>
      {/* Unavailable rather than disabled while the review could not be read again, so the button keeps focus and says why
          (the Alert above the review states the same), as its tooltip and description; a press takes focus to the Alert's
          button (see `apply`). The title stays the same in every state and `isDisabled` gates it: an empty title
          would drop the tooltip wrapper and remount the button, losing focus. */}
      <TitledControl key="import" title={t("configuration.import.refreshFailedTitle")} isDisabled={!stuck}>
        <Button
          ref={importButton}
          type="submit"
          pending={busy || refreshing}
          disabled={choices.nothingChosen}
          unavailable={stuck ? t("configuration.import.refreshFailedTitle") : undefined}
        >
          {t("configuration.import.review.submit")}
        </Button>
      </TitledControl>
    </>
  );

  const chooseFooter = (
    <>
      <Button key="cancel" type="button" variant="secondary" onPress={onClose}>
        {t("common.cancel")}
      </Button>
      {/* Pending rather than disabled, so the button keeps focus and the dialog its Escape. */}
      {/* Wrapped as the review's Import button is, so the same element carries on into the review. */}
      <TitledControl key="import" title={t("configuration.import.refreshFailedTitle")} isDisabled>
        <Button type="submit" pending={busy || waiting} autoFocus>
          {t("configuration.import.choose.submit")}
        </Button>
      </TitledControl>
    </>
  );

  const frame =
    stage.kind === "choose"
      ? { footer: chooseFooter, onSubmit: () => void choose() }
      : stage.kind === "review" || stage.kind === "importing"
        // The importing step keeps the form, so the pressed button is not remounted; `apply` does nothing there.
        ? { footer: importFooter, onSubmit: apply }
        : {
          footer: (
            <Button type="button" onPress={onClose}>
              {t("common.close")}
            </Button>
          ),
        };
  // The result's lines of the settings only the client holds, and the language the daemon applied.
  const resultLines = stage.kind === "result"
    ? [...clientSettingValues(t, stage.clientSettings), ...languageSettingValues(stage.result.settings_applied?.interface_language)]
    : [];

  return (
    <Dialog
      title={t(stage.kind === "result" ? "configuration.import.result.title" : "configuration.import.title")}
      onClose={onClose}
      busy={busy}
      size={stage.kind === "choose" ? "md" : "wide"}
      resetKey={stage.kind}
      {...frame}
    >
      <div ref={bodyRef} className="flex flex-col gap-4">
        {stage.kind === "choose" && <p className="text-sm"><Message id="configuration.import.choose.filePrompt" /></p>}
        {stage.kind === "review" && stuck && refreshFailure && (
          <div ref={refreshAlert} className="contents">
            <FailureAlert
              title={t("configuration.import.refreshFailedTitle")}
              message={refreshFailure.message}
              onRetry={() => void refresh(stage)}
              // A failure no retry can change is met by reading the file from the start.
              otherwise={{
                remedy: <Message id="configuration.import.refreshFailedChooseAgain" />,
                action: (
                  <AlertButton kind="action" size="sm" focusOnPress={false} pending={waiting} onPress={() => void choose()}>
                    {t("configuration.import.choose.submit")}
                  </AlertButton>
                ),
              }}
              pending={refreshing}
              announce
              announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}
              delayEachAttempt
              attempt={refreshFailure.attempt}
            />
          </div>
        )}
        {stage.kind === "review" && (
          <ImportReview
            fileName={stage.fileName}
            inspection={{ ...stage.inspection, consoles: choices.offered }}
            review={review}
            onReview={setReview}
            accountReview={accountReview}
            onAccountReview={setAccountReview}
            checked={choices.checked}
            onChecked={(part, on) => choices.change({ checked: { ...choices.checked, [part]: on } })}
            sectionsChecked={choices.sectionsChecked}
            onSectionChecked={(section, on) => choices.change({ sectionsChecked: { ...choices.sectionsChecked, [section]: on } })}
            tab={tab}
            onTab={setTab}
            shortcuts={shortcuts && shortcutsReview(shortcuts, currentShortcuts, host)}
            clientSettings={clientSettings.values}
            sorting={clientSettings.sorting}
            cloneDir={{ use: choices.useCloneDir, onUse: (use) => choices.change({ useCloneDir: use }) }}
            shown={shown}
          />
        )}
        {stage.kind === "importing" && <ImportProgressView inspection={stage.inspection} progress={progress} />}
        {stage.kind === "result" && <ImportResult result={stage.result} shortcuts={stage.shortcuts} clientSettings={resultLines} sorting={stage.clientSettings.session_order && sessionOrderValues(t, stage.clientSettings.session_order)} />}
        <ChangeAnnouncer count={refreshed} text={t("configuration.import.reviewUpdated")} />
      </div>
    </Dialog>
  );
}
