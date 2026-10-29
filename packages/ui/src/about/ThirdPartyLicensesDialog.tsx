import { SearchField, Surface } from "@heroui/react";
import { ChevronDown, ChevronRight, ScrollText, SearchX } from "lucide-react";
import React, { useDeferredValue, useEffect, useId, useState } from "react";

import { CodeBlock } from "../components/CodeBlock";
import { DiagnosticBlock } from "../components/DiagnosticBlock";
import { EmptyPanel } from "../components/EmptyPanel";
import { PaneFailure } from "../components/PaneFailure";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { Dialog } from "../dialogs/Dialog";
import { useTrimmedField } from "../dialogs/useTrimmedField";
import { useT } from "../i18n/react";
import { filterPackages, loadThirdPartyLicenses, type LicensedPackage, type ThirdPartyLicenses } from "./thirdPartyLicenses";

/** One package: its name, version and license on the summary of a disclosure, and its license texts behind it.
 * The texts are rendered only while it is open, so a list of hundreds costs no more than its summaries. The
 * summary is a flex row, which takes the browser's own marker away (WebKit's needs its pseudo-element hidden too), so
 * it draws a chevron of its own, in a column of its own so a wrapped second line stays under the name. */
function PackageRow({ entry, texts }: { entry: LicensedPackage; texts: readonly string[] }): React.ReactElement {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <li className="groove-b py-2 last:border-b-0">
      <details open={open} onToggle={(event) => setOpen(event.currentTarget.open)} className="group min-w-0">
        <summary className="flex cursor-pointer items-baseline gap-2 text-sm [&::-webkit-details-marker]:hidden">
          <ChevronRight aria-hidden="true" className="size-4 shrink-0 translate-y-0.5 self-start text-app-muted group-open:hidden rtl:-scale-x-100" />
          <ChevronDown aria-hidden="true" className="hidden size-4 shrink-0 translate-y-0.5 self-start text-app-muted group-open:block" />
          <span className="flex min-w-0 flex-1 flex-wrap items-baseline gap-x-3">
            <span dir="ltr" className="font-medium text-foreground [overflow-wrap:anywhere]">{entry.name} {entry.version}</span>
            <span dir="ltr" className="text-app-muted">{entry.license}</span>
            <span className="text-app-muted">{t(entry.ecosystem === "npm" ? "settings.about.thirdParty.npm" : "settings.about.thirdParty.cargo")}</span>
          </span>
        </summary>
        {open && (
          <div className="mt-2 flex min-w-0 flex-col gap-2">
            {entry.texts.length === 0 && <p className="text-sm text-app-muted">{t("settings.about.thirdParty.noText")}</p>}
            {entry.texts.map((index) => (
              <Surface key={index} variant="secondary" className="min-w-0 max-w-full rounded-md">
                <CodeBlock dir="ltr" className="[&>.app-code]:rounded-none [&>.app-code]:bg-transparent">{texts[index] ?? ""}</CodeBlock>
              </Surface>
            ))}
          </div>
        )}
      </details>
    </li>
  );
}

/** The packages the application ships, searchable by name or license, each with its license texts expandable. */
export function ThirdPartyLicensesList({ licenses }: { licenses: ThirdPartyLicenses }): React.ReactElement {
  const t = useT();
  const [query, setQuery] = useState("");
  const keywordField = useTrimmedField(query, setQuery);
  const shown = useDeferredValue(filterPackages(licenses.packages, query));
  const listId = useId();
  if (licenses.packages.length === 0) {
    return <EmptyPanel icon={ScrollText} message={t("settings.about.thirdParty.empty")} description={t("settings.about.thirdParty.empty.description")} />;
  }
  return (
    <>
      <SearchField aria-label={t("settings.about.thirdParty.search")} {...keywordField} fullWidth variant="secondary" aria-controls={shown.length === 0 ? undefined : listId}>
        <SearchField.Group>
          <SearchField.SearchIcon />
          <SearchField.Input placeholder={t("settings.about.thirdParty.placeholder")} />
          <SearchField.ClearButton aria-label={t("common.clear")} />
        </SearchField.Group>
      </SearchField>
      <p className="text-sm text-app-muted">{t("settings.about.thirdParty.count", { count: shown.length })}</p>
      <StatusAnnouncer text={query.trim() === "" ? undefined : shown.length === 0 ? t("settings.about.thirdParty.none.sentence") : t("settings.about.thirdParty.count", { count: shown.length })} />
      {shown.length === 0 ? (
        <EmptyPanel silent icon={SearchX} message={t("settings.about.thirdParty.none")} />
      ) : (
        <ul id={listId} className="m-0 list-none p-0">
          {shown.map((entry) => <PackageRow key={`${entry.ecosystem}:${entry.name}@${entry.version}`} entry={entry} texts={licenses.texts} />)}
        </ul>
      )}
    </>
  );
}

/** Where the dialog's load stands: running, ended in `error` (the loader's own text, for the failure's details), or
 * done. `retrying` is a Try again whose load is still running, which keeps the failure on screen with its button
 * pending. */
type Loaded = { status: "loading" } | { status: "failed"; error: string; retrying: boolean } | { status: "loaded"; licenses: ThirdPartyLicenses };

/** The third-party licenses, loaded when the dialog opens (the list is a chunk of its own, see
 * `loadThirdPartyLicenses`) and again on Try again after a failure. `load` says where from, for a surface that has
 * to show the dialog's loading, empty and failed states with data of its own. */
export function ThirdPartyLicensesDialog({ onClose, load = loadThirdPartyLicenses }: { onClose: () => void; load?: () => Promise<ThirdPartyLicenses> }): React.ReactElement {
  const t = useT();
  const [state, setState] = useState<Loaded>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let current = true;
    load().then(
      (licenses) => current && setState({ status: "loaded", licenses }),
      (error: unknown) => current && setState({ status: "failed", error: error instanceof Error ? error.message : String(error), retrying: false }),
    );
    return () => {
      current = false;
    };
  }, [attempt]);
  return (
    <Dialog
      title={t("settings.about.thirdParty.title")}
      subtitle={<p className="text-sm text-app-muted">{t("settings.about.thirdParty.intro")}</p>}
      size="wide"
      onClose={onClose}
      footer={null}
      resetKey={state.status}
    >
      {/* Mounted whatever the state, with the dialog, so the loading line is written into a region that already exists;
          its first write waits out the focus move of the dialog opening. */}
      <StatusAnnouncer text={state.status === "loading" ? t("settings.about.thirdParty.loading") : undefined} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />
      {state.status === "loading" ? <p aria-hidden="true" className="text-sm text-app-muted">{t("settings.about.thirdParty.loading")}</p>
        : state.status === "failed" ? (
          <PaneFailure
            title={t("settings.about.thirdParty.failed")}
            summary={t("settings.about.thirdParty.failed.summary")}
            remedy={t("settings.about.thirdParty.failed.remedy")}
            details={state.error ? <DiagnosticBlock>{state.error}</DiagnosticBlock> : undefined}
            pending={state.retrying}
            announce
            attempt={attempt}
            onRetry={() => {
              setState({ ...state, retrying: true });
              setAttempt(attempt + 1);
            }}
          />
        )
          : <ThirdPartyLicensesList licenses={state.licenses} />}
    </Dialog>
  );
}
