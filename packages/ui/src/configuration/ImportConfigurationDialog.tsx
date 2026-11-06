import React, { useEffect, useRef, useState } from "react";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { InlineReference } from "../i18n/InlineReference";
import { Dialog } from "../components/dialog/Dialog";
import { useDialogAction, useSubmitValidation } from "../components/dialog/dialogHooks";
import { dialogAround } from "../components/dialog/dialogElement";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { runInBackground } from "../lifecycle/runInBackground";
import { usePlatform } from "../platform/react";
import { useOctoboardTheme } from "../preferences/theme";
import { markdownStyle } from "../viewer/markdown/markdownStyle";
import { replaceOverrides, shortcutOverrides, type Overrides } from "../shortcuts/store";
import { useDaemon } from "../store";
import { applyClientSettings, clientSettingsOf, clientSettingValues, documentClientSettings, type ClientSettings } from "./clientSettings";
import { fileFailure } from "./failureText";
import {
  accountResolutionsOf,
  carriedRegions,
  documentShortcuts,
  hasAccountDirectoryProblem,
  hasDirectoryProblem,
  importedRegions,
  initialAccountReview,
  initialReview,
  resolutionsOf,
  reviewForOffers,
  shortcutsRecord,
  shortcutsReview,
  type AccountReviewState,
  type Inspection,
  type ReviewState,
  withCloneDirOffers,
} from "./importPlan";
import { ImportProgressView } from "./ImportProgress";
import { newImportId, NO_PROGRESS, withProgress, type ImportProgress } from "./progressState";
import { ImportResult, type Imported } from "./ImportResult";
import { ImportReview } from "./ImportReview";
import type { Region } from "./RegionTabs";
import { raiseError } from "../components/alerts/raiseError";
import { Button } from "../primitives";

type Stage =
  | { kind: "choose" }
  | { kind: "review"; fileName: string; document: unknown; inspection: Inspection }
  | { kind: "importing"; inspection: Inspection }
  | { kind: "result"; result: Imported; shortcuts?: { applied: number; skipped: number; overrides: Overrides }; clientSettings: ClientSettings };

/** How long after the window regains focus a file may still arrive before the picker is taken as
 * dismissed: a browser reports the chosen file just after the focus returns. */
const FOCUS_GRACE_MS = 500;

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

/** The regions checked on opening a review: the application settings change how this machine behaves rather than add
 * something to it, so they start unchecked. */
const DEFAULT_CHECKED: Record<Region, boolean> = { consoles: true, accounts: true, settings: false };

/** `DEFAULT_CHECKED` for `inspection`, except that an accounts region bringing nothing to the accounts (one that carries
 * only the Agent accounts settings, as a file of an earlier version may) starts unchecked like the settings: there is
 * nothing in it to add. What it brings is an agent to add, an account, or a default account's name or switch that
 * applies. */
function checkedOnOpening(inspection: Inspection): Record<Region, boolean> {
  const { accounts } = inspection;
  const adds =
    !!accounts &&
    (accounts.agents_to_add.length > 0 ||
      accounts.agents.some((agent) => agent.accounts.length > 0 || (agent.default_name_applies && !!agent.default_name) || agent.default_disabled_applies));
  return { ...DEFAULT_CHECKED, accounts: adds };
}

/**
 * Imports a configuration document in three steps inside one dialog. The user picks a file through the platform's open
 * dialog and the daemon inspects it; what it answers is reviewed here, a row for every project and for every account,
 * before anything is created; and applying sends the document as it was read with the choice made for each project and
 * account, and shows what came of it: while the daemon works, the body lists every item being imported with the state
 * the daemon's progress reports give it (`ImportProgressView`), and the result takes its place when the reply comes.
 * The regions of the file are imported as the user checks them (the consoles and projects and the accounts by default,
 * the application settings not, nor accounts that bring nothing to the accounts), and only a checked region is sent,
 * reviewed or shown in the result; with none checked there is nothing to import. A directory typed for a project or an
 * account that the daemon would refuse stops the import at the row, bringing its tab forward. A document the daemon
 * refuses leaves the dialog on the first step, with the reason, for another file to be tried. The settings only this
 * client holds (appearance, Markdown style, running in the background, language) and the keyboard shortcuts travel in
 * the application settings: the review says what each would change here, and they are applied here from the daemon's
 * reply, for what it says it applied. The file's default clone directory is applied unless the user keeps this
 * machine's own. The daemon's reply to an import comes only after its last clone, so the dialog cannot be dismissed
 * while one is out. Waiting for the file is not such a time: a browser without the file input's `cancel` event never
 * answers for a dismissed picker, so the dialog is held busy only from the file onwards. The button shows it is
 * waiting and ignores another press meanwhile, but Escape and the close button still work. Where the picker is never
 * reported dismissed, the window regaining focus with no file chosen frees the step to be tried again; a file that
 * arrives from that picker after all is still honoured, until a newer press opens another one.
 */
export function ImportConfigurationDialog({ onClose }: { onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request, onImportProgress } = useDaemon();
  const { configurationFile, backgroundRun } = usePlatform();
  const { choice: appearance, setChoice: setAppearance } = useOctoboardTheme();
  const language = useCurrentLanguage();
  const markdown = markdownStyle.useValue();
  const inBackground = runInBackground.useValue();
  const { busy, run } = useDialogAction(undefined, t("configuration.import.alertTitle"));
  const { shown, attempt } = useSubmitValidation();
  const [stage, setStage] = useState<Stage>({ kind: "choose" });
  const [review, setReview] = useState<ReviewState>([]);
  const [accountReview, setAccountReview] = useState<AccountReviewState>([]);
  // Which regions of the file are imported: an unchecked one has no tab and is not sent. Read only for a file that
  // carries more than one (see `importedRegions`).
  const [checked, setChecked] = useState<Record<Region, boolean>>(DEFAULT_CHECKED);
  // The review's selected tab, kept here so an import stopped by a row's problem can bring the row's tab forward.
  const [tab, setTab] = useState<Region>();
  // Whether the file's default clone directory is applied with the application settings, or this machine's own is kept.
  const [useCloneDir, setUseCloneDir] = useState(true);
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
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("focus", onFocus);
      clearTimeout(timer);
    };
  }, [waiting]);
  const currentShortcuts = shortcutOverrides.useValue();
  const shortcuts = stage.kind === "review" ? documentShortcuts(stage.document) : undefined;
  const platformSettings = { backgroundRun: backgroundRun !== undefined };
  const clientSettings = stage.kind === "review"
    ? clientSettingValues(t, documentClientSettings(stage.document, platformSettings), {
      appearance,
      markdown_style: markdown,
      ...(platformSettings.backgroundRun && { run_in_background: inBackground }),
      interface_language: language,
    })
    : [];
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
      setChecked(checkedOnOpening(inspection));
      setUseCloneDir(true);
      setTab(undefined);
      setStage({ kind: "review", fileName: opened.name, document, inspection });
    });
  };

  // The targets the review offers follow the choice about the default clone directory: under the file's while it is
  // applied (the settings imported and the box checked), else under this machine's.
  const takesDocumentCloneDir = (inspection: Inspection, regions: Record<Region, boolean>, use: boolean) =>
    use && importedRegions(carriedRegions(inspection), regions).includes("settings");
  const reviewedConsoles =
    stage.kind === "review" ? withCloneDirOffers(stage.inspection.consoles, takesDocumentCloneDir(stage.inspection, checked, useCloneDir)) : [];
  // Changes what is checked or the clone directory choice, moving the rows whose offers that changes.
  const changeBasis = (nextChecked: Record<Region, boolean>, nextUse: boolean) => {
    if (stage.kind === "review") {
      const next = withCloneDirOffers(stage.inspection.consoles, takesDocumentCloneDir(stage.inspection, nextChecked, nextUse));
      setReview(reviewForOffers(review, next));
    }
    setChecked(nextChecked);
    setUseCloneDir(nextUse);
  };

  const apply = () => {
    if (stage.kind !== "review" || busy) return;
    const accounts = stage.inspection.accounts;
    const inFile = carriedRegions(stage.inspection);
    const regions = importedRegions(inFile, checked);
    if (regions.length === 0) return;
    const importingConsoles = regions.includes("consoles");
    const creatingAccounts = regions.includes("accounts") && !!accounts;
    const consolesProblem = importingConsoles && hasDirectoryProblem(review);
    const accountsProblem = creatingAccounts && hasAccountDirectoryProblem(accountReview);
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
        apply_settings: regions.includes("settings"),
        // Absent means true, so only a refusal is spelled out.
        ...(inFile.includes("consoles") && !importingConsoles && { apply_consoles: false }),
        ...(regions.includes("settings") && !useCloneDir && { apply_clone_dir: false }),
        ...(creatingAccounts && { apply_accounts: true, account_resolutions: accountResolutionsOf(accounts, accountReview) }),
      });
    void run(async () => {
      setProgress(NO_PROGRESS);
      setStage(importing);
      const result = await whileListening(
        () => onImportProgress(importId, (event) => setProgress((current) => withProgress(current, event))),
        send,
      ).catch((err: unknown) => {
        // The request failed, whether the daemon refused it or the connection closed under it: back to the review,
        // which still holds every choice, and `run` reports the failure.
        setStage(reviewing);
        throw err;
      });
      // The daemon applied the settings it knows and echoed the ones it passes through, which are this client's own
      // to apply: it does so only for what the daemon says it applied.
      const carried = shortcutsRecord(result.settings_applied?.keyboard_shortcuts);
      const applied = carried && replaceOverrides(carried);
      const clientApplied = clientSettingsOf(result.settings_applied, { backgroundRun: backgroundRun !== undefined });
      applyClientSettings(clientApplied, setAppearance);
      setStage({ kind: "result", result, shortcuts: applied, clientSettings: clientApplied });
    });
  };

  // Nothing checked is nothing to import.
  const nothingToImport = stage.kind === "review" && importedRegions(carriedRegions(stage.inspection), checked).length === 0;
  // The footer's buttons stay while the body changes: the same keys in every step's footer keep each one mounted. The
  // import step shows this footer too: the daemon has no way to stop an import, so Cancel is disabled beside the
  // pending Import, and the Import button that was pressed stays the same element and keeps focus as it turns pending.
  const importFooter = (
    <>
      <Button key="cancel" type="button" variant="secondary" onPress={onClose}>
        {t("common.cancel")}
      </Button>
      <Button key="import" type="submit" pending={busy} disabled={nothingToImport}>
        {t("configuration.import.review.submit")}
      </Button>
    </>
  );

  const chooseFooter = (
    <>
      <Button key="cancel" type="button" variant="secondary" onPress={onClose}>
        {t("common.cancel")}
      </Button>
      {/* Pending rather than disabled, so the button keeps focus and the dialog its Escape. */}
      <Button key="import" type="submit" pending={busy || waiting} autoFocus>
        {t("configuration.import.choose.submit")}
      </Button>
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
        {stage.kind === "review" && (
          <ImportReview
            fileName={stage.fileName}
            inspection={{ ...stage.inspection, consoles: reviewedConsoles }}
            review={review}
            onReview={setReview}
            accountReview={accountReview}
            onAccountReview={setAccountReview}
            checked={checked}
            onChecked={(region, on) => changeBasis({ ...checked, [region]: on }, useCloneDir)}
            tab={tab}
            onTab={setTab}
            shortcuts={shortcuts && shortcutsReview(shortcuts, currentShortcuts)}
            clientSettings={clientSettings}
            cloneDir={{ use: useCloneDir, onUse: (use) => changeBasis(checked, use) }}
            shown={shown}
          />
        )}
        {stage.kind === "importing" && <ImportProgressView inspection={stage.inspection} progress={progress} />}
        {stage.kind === "result" && <ImportResult result={stage.result} shortcuts={stage.shortcuts} clientSettings={clientSettingValues(t, stage.clientSettings)} />}
      </div>
    </Dialog>
  );
}
