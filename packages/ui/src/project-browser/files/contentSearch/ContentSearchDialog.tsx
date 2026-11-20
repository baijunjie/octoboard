import { ChevronRight, SearchX } from "lucide-react";
import React, { useId, useRef, useState } from "react";

import { FadeOverflow } from "../../../components/text/FadeOverflow";
import { DaemonErrorMessage } from "../../../components/alerts/DaemonErrorMessage";
import { PaneFailure } from "../../../components/alerts/PaneFailure";
import { StatusAnnouncer } from "../../../components/announce/StatusAnnouncer";
import { FieldClearButton } from "../../../components/controls/FieldClearButton";
import { FieldErrorMessage } from "../../../components/form/FieldErrorMessage";
import { FieldRow } from "../../../components/form/FieldRow";
import { Dialog } from "../../../components/dialog/Dialog";
import { handFocusOff } from "../../../components/focus/handFocusOff";
import { useIsNarrow } from "../../../components/hooks/breakpoint";
import { CenteredEmptyPanel } from "../../../components/panel/EmptyPanel";
import { LoadingState } from "../../../components/panel/LoadingState";
import { StatusMark } from "../../../components/status/StatusMark";
import type { ErrorPresentation } from "../../../errors/errorPresentation";
import { displayWirePath } from "../../../format/wirePath";
import type { Translate } from "../../../i18n/catalog";
import { InlineReference } from "../../../i18n/InlineReference";
import { isolate } from "../../../i18n/isolate";
import { Message, useT } from "../../../i18n/react";
import { isImeKey } from "../../../input/imeKey";
import { Input, SearchField, Spinner, TextField } from "../../../primitives";
import type { ContentMatch } from "../../../protocol";
import { FindOptionToggles } from "../../../viewer/find/FindBar";
import { IncludeIgnoredToggle } from "../../IncludeIgnoredToggle";
import { ContentSearchPreview } from "./ContentSearchPreview";
import { ContentSearchResults } from "./ContentSearchResults";
import { ContentSearchSplit } from "./ContentSearchSplit";
import { matchKey } from "./matchText";
import { usePlaceholderWidth } from "./placeholderWidth";
import { useContentSearch, type ContentSearch, type ContentSearchFound, type ContentSearchOptions } from "./useContentSearch";

/** The file mask's example, the daemon's syntax: comma-separated globs, one starting with `!` leaving files out. Code,
 * the same in every language. */
const FILE_MASK_EXAMPLE = "*.ts, !*.test.ts";

/** The count of what a search has found, "12 matches in 3 files". */
function countText(t: Translate, found: ContentSearchFound): string {
  return t("contentSearch.count", { count: found.matchCount, files: t("contentSearch.files", { count: found.fileCount }) });
}

/** What is said once a search has ended: that nothing was found, or the count, and that it is incomplete when it is. */
function endText(t: Translate, found: ContentSearchFound): string {
  if (found.matches.length === 0) return t(found.complete ? "contentSearch.noMatch" : "contentSearch.noMatch.incomplete.sentence");
  return found.complete ? countText(t, found) : t("contentSearch.countIncomplete", { found: countText(t, found) });
}

/** Why a search that ended is incomplete, for its mark's popover. */
function incompleteDetails(t: Translate, found: ContentSearchFound): string {
  switch (found.stoppedBy) {
    case "time": return t("contentSearch.partial.time");
    case "matches": return t("contentSearch.partial.matches");
    case "bytes_read": return t("contentSearch.partial.bytesRead");
    case "result_bytes": return t("contentSearch.partial.resultBytes");
    case undefined: return t("contentSearch.partial.unreadable");
  }
}

/** The daemon's refusal of the query or the file mask, under the field it refuses (`FieldErrorMessage`, announced as an
 * alert): one sentence in the user's language, worded from why it was refused, a refused glob set apart as a
 * reference; nothing while `error` is absent. */
function RefusalMessage({ error }: { error: ErrorPresentation | undefined }): React.ReactElement {
  return (
    <FieldErrorMessage>
      {error !== undefined && (
        <span data-content-search-refusal="" className="[overflow-wrap:anywhere]">
          <DaemonErrorMessage error={error} part="summary" />
        </span>
      )}
    </FieldErrorMessage>
  );
}

/** The selected result: the one chosen while it is still listed, otherwise the first. */
function selectedOf(result: ContentSearch, chosen: string | null): ContentMatch | undefined {
  if (result.state !== "searching" && result.state !== "done") return undefined;
  return (chosen !== null ? result.matches.find((match) => matchKey(match) === chosen) : undefined) ?? result.matches[0];
}

/**
 * The project's Find in files modal. Under its title, the scope (the project, and the folder after it when opened on a
 * `directory`, which the modal offers no way to change), with the count of matches and files at the end of that line;
 * then the query, with the Match case, Whole word and Use regular expression toggles in it, a file mask and the
 * include-ignored toggle; and under them the result list, one row per matching line, filled as the daemon streams them
 * (`useContentSearch`). Opening a result (Enter, a double click, or Enter in the query for the selected one) calls
 * `onOpen`, whose caller closes the modal and shows the file at its line.
 *
 * Under the list, the selected result's file is previewed at its line (`ContentSearchPreview`). At narrow and phone
 * widths the modal fills the screen and holds the list alone.
 */
export function ContentSearchDialog({ project, projectName, directory, query, options, onQueryChange, onOptionsChange, onOpen, onClose }: {
  project: string;
  /** The project's name, shown as the search's scope. */
  projectName: string;
  /** The wire path of the directory the search is limited to; the whole project when absent. */
  directory?: string;
  query: string;
  options: ContentSearchOptions;
  onQueryChange: (query: string) => void;
  onOptionsChange: (options: ContentSearchOptions) => void;
  onOpen: (match: ContentMatch) => void;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const fullScreen = useIsNarrow();
  const { result, retry } = useContentSearch({ project, directory, query, options });
  const [chosen, setChosen] = useState<string | null>(null);
  const selected = selectedOf(result, chosen);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const maskRef = useRef<HTMLInputElement>(null);
  // The example reads left to right inside the sentence of any language.
  const maskPlaceholder = t("contentSearch.fileMask.placeholder", { example: isolate(FILE_MASK_EXAMPLE) });
  const maskPlaceholderWidth = usePlaceholderWidth(maskRef, maskPlaceholder);
  const maskHelpId = useId();
  const invalid = result.state === "invalid" ? result : undefined;
  const found = result.state === "searching" || result.state === "done" ? result : undefined;

  const onQueryKeyDown = (event: React.KeyboardEvent) => {
    if (event.target !== inputRef.current || isImeKey(event.nativeEvent)) return;
    if (event.key === "ArrowDown" && found && found.matches.length > 0) {
      event.preventDefault();
      listRef.current?.focus();
    } else if (event.key === "Enter" && selected) {
      event.preventDefault();
      onOpen(selected);
    }
  };

  const spoken = result.state === "done" ? endText(t, result) : undefined;
  // The line keeps its chips on one line for the fade to clip; its tooltip lets them wrap, a path at its slashes.
  const scope = (wrap: boolean) => (
    <Message
      id="contentSearch.scope"
      params={{
        scope: (
          <>
            <InlineReference dir="auto" wrap={wrap}>{projectName}</InlineReference>
            {/* A location rather than a run of names, so each part keeps its own chip (the folder the monospace one a
                path takes), joined as the title bar's breadcrumb is: an exception to one chip per joined run, settled
                with the user. A screen reader hears a slash where the chevron is drawn. */}
            {directory !== undefined && (
              <>
                <ChevronRight className="mx-0.5 inline size-3 align-middle rtl:-scale-x-100" />
                <span className="sr-only"> / </span>
                <InlineReference monospace wrap={wrap}>{displayWirePath(directory)}</InlineReference>
              </>
            )}
          </>
        ),
      }}
    />
  );
  return (
    <Dialog
      size="viewer"
      fullScreen={fullScreen}
      title={t("contentSearch.title")}
      subtitle={(
        // The scope at the start, always, so the user knows which project is searched; what the search has found at the
        // end of the same line, which keeps the fields' row free of a line held empty until results come. The line is
        // one line of text, whatever it holds: a scope too long for the room the count leaves fades at its end and
        // shows in full in its tooltip, as the file viewer's subtitle does, so the fields below never move as the count
        // comes and goes.
        <div className="col-span-full flex min-w-0 items-center justify-between gap-x-4 text-xs text-app-muted">
          <div data-content-search-scope="" className="flex min-w-0 flex-1">
            <FadeOverflow as="span" className="min-w-0" titleWhenClipped={scope(true)}>
              {scope(false)}
            </FadeOverflow>
          </div>
          {/* The spinner and the incomplete mark stand over the line rather than make it taller. */}
          <div className="flex h-4 shrink-0 items-center gap-1.5" data-content-search-status="">
            {found && (found.matches.length > 0 || (found.state === "done" && !found.complete)) ? (
              // Before the first result the list's place says the search is running; at the end, a search that found
              // nothing says so there, with the count and the mark kept here only when it is incomplete.
              <>
                {found.state === "searching" && <span data-content-search-spinner="" className="flex"><Spinner size="sm" /></span>}
                <span className="tabular-nums">{countText(t, found)}</span>
                {found.state === "done" && !found.complete && (
                  <StatusMark tone="warning" label={t("contentSearch.partial")}>{incompleteDetails(t, found)}</StatusMark>
                )}
              </>
            ) : null}
          </div>
        </div>
      )}
      onClose={onClose}
      footer={null}
    >
      {/* The top padding sets the fields apart from the header (it also holds their focus ring, which the body would clip
          at its top edge). */}
      <div className="flex min-w-0 shrink-0 flex-col groove-b px-6 pt-4 pb-4 max-tablet:px-3">
        {/* Down and Enter in the query, caught before the field's own handler: keys an input method composes are its own,
            and none of them moves to the list or opens a result. */}
        {/* Lined up at their tops: a refusal under one field leaves the other where it is. */}
        <div className="flex min-w-0 flex-wrap items-start gap-2" onKeyDownCapture={onQueryKeyDown}>
          <SearchField
            label={t("contentSearch.query")}
            value={query}
            onChange={onQueryChange}
            autoFocus
            fullWidth
            className="min-w-48 flex-[3_1_16rem]"
            inputRef={inputRef}
            placeholder={t("common.search")}
            invalid={invalid?.field === "query"}
            trailing={<FindOptionToggles options={options} onOptionsChange={onOptionsChange} />}
            clearButton={<FieldClearButton />}
            error={<RefusalMessage error={invalid?.field === "query" ? invalid.error : undefined} />}
          />
          {/* The mask's row is never narrower than its placeholder needs, the toggle and the gap beside it included, so the
              row wraps before the example is cut. */}
          <div className="flex min-w-0 flex-[1_1_10rem]" style={maskPlaceholderWidth === undefined ? undefined : { minWidth: `min(100%, ${maskPlaceholderWidth}px)` }}>
            {/* The field holds the row, so its error stays under the input and the toggle both (as `TextInput`'s
                `trailing`). */}
            <TextField
              label={t("contentSearch.fileMask")}
              describedBy={maskHelpId}
              invalid={invalid?.field === "fileMask"}
              value={options.fileMask}
              onChange={(fileMask) => onOptionsChange({ ...options, fileMask })}
              fullWidth
              className="min-w-0 flex-1"
            >
              <FieldRow>
                <Input
                  ref={maskRef}
                  data-1p-ignore
                  placeholder={maskPlaceholder}
                  // A mask reads left to right; the placeholder, a sentence, in the interface's direction while it shows.
                  dir={options.fileMask === "" ? undefined : "ltr"}
                  spellCheck={false}
                  autoComplete="off"
                  autoCapitalize="off"
                  autoCorrect="off"
                  fullWidth
                  className="min-w-0 flex-1"
                />
                <span id={maskHelpId} hidden>{t("contentSearch.fileMask.help")}</span>
                <IncludeIgnoredToggle included={options.includeIgnored} onChange={(includeIgnored) => onOptionsChange({ ...options, includeIgnored })} />
              </FieldRow>
              <RefusalMessage error={invalid?.field === "fileMask" ? invalid.error : undefined} />
            </TextField>
          </div>
        </div>
      </div>
      {/* A refusal is announced by its field's alert. */}
      <StatusAnnouncer text={spoken} />
      <div className="flex min-h-0 flex-1 flex-col bg-overlay">
        {result.state === "error" ? (
          <PaneFailure
            title={t("contentSearch.error")}
            message={result.message}
            announce
            onRetry={(from) => {
              handFocusOff(from, inputRef.current, true);
              void retry();
            }}
          />
        ) : found && found.matches.length > 0 ? (
          <ContentSearchSplit
            list={(
              <ContentSearchResults
                matches={found.matches}
                selectedId={selected ? matchKey(selected) : null}
                showFolder={fullScreen}
                onSelectedChange={setChosen}
                onOpen={onOpen}
                listRef={listRef}
              />
            )}
            // The selected result's file below the list; a narrow window has room for the list alone.
            preview={fullScreen ? null : <ContentSearchPreview project={project} selected={selected} />}
          />
        ) : found?.state === "searching" ? (
          <LoadingState label={t("contentSearch.searching")} />
        ) : found?.state === "done" ? (
          <CenteredEmptyPanel icon={SearchX} message={t("contentSearch.noMatch")} description={found.complete ? undefined : t("contentSearch.noMatch.incomplete.description")} silent />
        ) : null}
      </div>
    </Dialog>
  );
}
