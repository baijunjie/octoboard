import { Button } from "../components/Button";
import { reportError } from "../reportError";

import React, { useEffect, useRef, useState } from "react";

import { errorAlerts } from "../errorAlerts";
import { InlineReference } from "../components/InlineReference";
import { Dialog, useDialogAction, useSubmitValidation } from "../dialogs/Dialog";
import { dialogAround } from "../dialogs/dialogElement";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { replaceOverrides, shortcutOverrides } from "../shortcuts/store";
import { useDaemon } from "../store";
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
  shortcutsRecord,
  shortcutsReview,
  type AccountReviewState,
  type Inspection,
  type ReviewState,
} from "./importPlan";
import { ImportResult, type Imported } from "./ImportResult";
import { ImportReview } from "./ImportReview";
import type { Region } from "./RegionTabs";

type Stage =
  | { kind: "choose" }
  | { kind: "review"; fileName: string; document: unknown; inspection: Inspection }
  | { kind: "result"; result: Imported; shortcuts?: { applied: number; skipped: number } };

/** How long after the window regains focus a file may still arrive before the picker is taken as
 * dismissed: a browser reports the chosen file just after the focus returns. */
const FOCUS_GRACE_MS = 500;

/** The regions checked on opening a review: the application settings change how this machine behaves rather than add
 * something to it, so they start unchecked. */
const DEFAULT_CHECKED: Record<Region, boolean> = { consoles: true, accounts: true, settings: false };

/**
 * Imports a configuration document in three steps inside one dialog. The user picks a file through the platform's open
 * dialog and the daemon inspects it; what it answers is reviewed here, a row for every project and for every account,
 * before anything is created; and applying sends the document as it was read with the choice made for each project and
 * account, and shows what came of it. The regions of the file are imported as the user checks them (the consoles and
 * projects and the accounts by default, the application settings not), and only a checked region is sent, reviewed or
 * shown in the result; with none checked there is nothing to import. A directory typed for a project or an account that
 * the daemon would refuse stops the import at the row, bringing its tab forward. A document the daemon refuses leaves
 * the dialog on the first step, with the reason, for another file to be tried. The daemon's reply to an import comes
 * only after its last clone, so the dialog cannot be dismissed while one is out. Waiting for the file is not such a
 * time: a browser without the file input's `cancel` event never answers for a dismissed picker, so the dialog is held
 * busy only from the file onwards. The button shows it is waiting and ignores another press meanwhile, but Escape and
 * the close button still work. Where the picker is never reported dismissed, the window regaining focus with no file
 * chosen frees the step to be tried again; a file that arrives from that picker after all is still honoured, until a
 * newer press opens another one.
 */
export function ImportConfigurationDialog({ onClose }: { onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const { configurationFile } = usePlatform();
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
  // Counts the imports a row's problem stopped, so the first such row gets focus once its message
  // is on screen.
  const [stopped, setStopped] = useState(0);
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
    let opened;
    try {
      opened = await configurationFile.open();
    } catch (err) {
      if (picker === pickerRef.current) reportError(fileFailure(t, err, "configuration.import.couldNotRead"), undefined, undefined, undefined, t("configuration.import.alertTitle"));
      return;
    } finally {
      if (picker === pickerRef.current) {
        waitingRef.current = false;
        setWaiting(false);
      }
    }
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
      const inspection = (await request({ type: "inspect_configuration", document })) as Inspection;
      setReview(initialReview(inspection.consoles));
      setAccountReview(initialAccountReview(inspection.accounts));
      setChecked(DEFAULT_CHECKED);
      setTab(undefined);
      setStage({ kind: "review", fileName: opened.name, document, inspection });
    });
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
    void run(async () => {
      const result = (await request({
        type: "import_configuration",
        document: stage.document,
        resolutions: importingConsoles ? resolutionsOf(review) : [],
        apply_settings: regions.includes("settings"),
        // Absent means true, so only a refusal is spelled out.
        ...(inFile.includes("consoles") && !importingConsoles && { apply_consoles: false }),
        ...(creatingAccounts && { apply_accounts: true, account_resolutions: accountResolutionsOf(accounts, accountReview) }),
      })) as Imported;
      // The daemon applied the settings it knows and echoed the shortcuts it passes through, which are this
      // client's own to apply: it does so only for what the daemon says it applied.
      const carried = shortcutsRecord(result.settings_applied?.keyboard_shortcuts);
      const applied = carried && replaceOverrides(carried);
      setStage({ kind: "result", result, shortcuts: applied });
    });
  };

  // Nothing checked is nothing to import.
  const nothingToImport = stage.kind === "review" && importedRegions(carriedRegions(stage.inspection), checked).length === 0;
  // The footer's buttons stay while the body changes.
  const reviewFooter = (
    <>
      <Button type="button" variant="secondary" onPress={onClose} isDisabled={busy}>
        {t("common.cancel")}
      </Button>
      <Button type="submit" isPending={busy} isDisabled={nothingToImport}>
        {busy ? t("configuration.import.review.pending") : t("configuration.import.review.submit")}
      </Button>
    </>
  );

  const chooseFooter = (
    <>
      <Button type="button" variant="secondary" onPress={onClose} isDisabled={busy}>
        {t("common.cancel")}
      </Button>
      {/* Pending rather than disabled, so the button keeps focus and the dialog its Escape. */}
      <Button type="submit" isPending={busy || waiting} autoFocus>
        {t("configuration.import.choose.submit")}
      </Button>
    </>
  );

  const frame =
    stage.kind === "choose"
      ? { footer: chooseFooter, onSubmit: () => void choose() }
      : stage.kind === "review"
        ? { footer: reviewFooter, onSubmit: apply }
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
      onClose={busy ? () => {} : onClose}
      size={stage.kind === "choose" ? "md" : "wide"}
      resetKey={stage.kind}
      {...frame}
    >
      <div ref={bodyRef} className="flex flex-col gap-4">
        {stage.kind === "choose" && <p className="text-sm"><Message id="configuration.import.choose.intro" /></p>}
        {stage.kind === "review" && (
          <ImportReview
            fileName={stage.fileName}
            inspection={stage.inspection}
            review={review}
            onReview={setReview}
            accountReview={accountReview}
            onAccountReview={setAccountReview}
            checked={checked}
            onChecked={(region, on) => setChecked((current) => ({ ...current, [region]: on }))}
            tab={tab}
            onTab={setTab}
            shortcuts={shortcuts && shortcutsReview(shortcuts, currentShortcuts)}
            shown={shown}
          />
        )}
        {stage.kind === "result" && <ImportResult result={stage.result} shortcuts={stage.shortcuts} />}
      </div>
    </Dialog>
  );
}
