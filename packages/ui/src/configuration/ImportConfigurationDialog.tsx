import { Button } from "@heroui/react";
import React, { useEffect, useRef, useState } from "react";

import { InlineCode } from "../components/InlineCode";
import { Dialog, DialogError, useDialogAction, useSubmitValidation } from "../dialogs/Dialog";
import { Message, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useDaemon } from "../store";
import { fileFailureText } from "./failureText";
import { hasDirectoryProblem, initialReview, resolutionsOf, type ReviewState } from "./importPlan";
import { ImportResult, type Imported } from "./ImportResult";
import { ImportReview, type Inspection } from "./ImportReview";

type Stage =
  | { kind: "choose" }
  | { kind: "review"; fileName: string; document: unknown; inspection: Inspection }
  | { kind: "result"; result: Imported };

/** How long after the window regains focus a file may still arrive before the picker is taken as
 * dismissed: a browser reports the chosen file just after the focus returns. */
const FOCUS_GRACE_MS = 500;

/**
 * Imports a configuration document in three steps inside one dialog. The user picks a file through
 * the platform's open dialog and the daemon inspects it; what it answers is reviewed here, a row
 * for every project, before anything is created; and applying sends the document as it was read
 * with the choice made for each project, and shows what came of it. A document the daemon refuses
 * leaves the dialog on the first step, with the reason, for another file to be tried. The daemon's
 * reply to an import comes only after its last clone, so the dialog cannot be dismissed while one
 * is out. Waiting for the file is not such a time: a browser without the file input's `cancel`
 * event never answers for a dismissed picker, so the dialog is held busy only from the file
 * onwards. The button shows it is waiting and ignores another press meanwhile, but Escape and the
 * close button still work. Where the picker is never reported dismissed, the window regaining focus
 * with no file chosen frees the step to be tried again; a file that arrives from that picker after
 * all is still honoured, until a newer press opens another one.
 */
export function ImportConfigurationDialog({ onClose }: { onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const { configurationFile } = usePlatform();
  const { error, setError, busy, run } = useDialogAction();
  // The name of a file that is not JSON, which its failure line sets as code.
  const [notJson, setNotJson] = useState<string>();
  const failure = notJson === undefined ? error : <Message id="configuration.import.notJson" params={{ name: <InlineCode>{notJson}</InlineCode> }} />;
  const { shown, attempt } = useSubmitValidation();
  const [stage, setStage] = useState<Stage>({ kind: "choose" });
  const [review, setReview] = useState<ReviewState>([]);
  const [applySettings, setApplySettings] = useState(false);
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
  const bodyRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (stopped > 0) bodyRef.current?.querySelector<HTMLElement>("input[aria-invalid=true]")?.focus();
  }, [stopped]);

  const choose = async () => {
    if (!configurationFile || busy || waitingRef.current) return;
    setError(undefined);
    setNotJson(undefined);
    const picker = ++pickerRef.current;
    waitingRef.current = true;
    setWaiting(true);
    let opened;
    try {
      opened = await configurationFile.open();
    } catch (err) {
      if (picker === pickerRef.current) setError(fileFailureText(t, err, "configuration.import.readFailed"));
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
        setNotJson(opened.name);
        return;
      }
      const inspection = (await request({ type: "inspect_configuration", document })) as Inspection;
      setReview(initialReview(inspection.consoles));
      setApplySettings(false);
      setStage({ kind: "review", fileName: opened.name, document, inspection });
    });
  };

  const apply = () => {
    if (stage.kind !== "review" || busy) return;
    if (!attempt(hasDirectoryProblem(review) ? "directory" : undefined)) {
      setStopped((count) => count + 1);
      return;
    }
    void run(async () => {
      const result = (await request({
        type: "import_configuration",
        document: stage.document,
        resolutions: resolutionsOf(review),
        apply_settings: applySettings,
      })) as Imported;
      setStage({ kind: "result", result });
    });
  };

  // The footer's buttons stay while the body changes, and the error sits in the footer so it is
  // seen without scrolling a long review back to its end.
  const reviewFooter = (
    <>
      {failure && (
        <div className="basis-full">
          <DialogError message={failure} />
        </div>
      )}
      <Button type="button" variant="secondary" onPress={onClose} isDisabled={busy}>
        {t("common.cancel")}
      </Button>
      <Button type="submit" isPending={busy}>
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
      <Button type="submit" isPending={busy || waiting}>
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
      title={t("configuration.import.title")}
      onClose={busy ? () => {} : onClose}
      size={stage.kind === "choose" ? "md" : "wide"}
      resetKey={stage.kind}
      {...frame}
    >
      <div ref={bodyRef} className="flex flex-col gap-4">
        {stage.kind === "choose" && (
          <>
            <p className="text-sm">{t("configuration.import.choose.intro")}</p>
            <DialogError message={failure} />
          </>
        )}
        {stage.kind === "review" && (
          <ImportReview
            fileName={stage.fileName}
            inspection={stage.inspection}
            review={review}
            onReview={setReview}
            applySettings={applySettings}
            onApplySettings={setApplySettings}
            shown={shown}
          />
        )}
        {stage.kind === "result" && <ImportResult result={stage.result} />}
      </div>
    </Dialog>
  );
}
