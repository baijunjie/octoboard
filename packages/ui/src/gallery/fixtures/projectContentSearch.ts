// The gallery's Find in files scenarios: the modal over a project's files (the fixture daemon searches the files'
// text, so the lines a result shows are the lines the preview reads), the states a search can end in, a search
// limited to a folder, and the file a result opens in the viewer.
import type { Ui } from "../../fixtures/interact";
import type { Scenario } from "../scenario";
import { SAMPLE, snapshotState } from "../../fixtures/builders";
import { SAMPLE_FILES, sourceUnavailable, type FixtureFiles } from "../../fixtures/sampleFiles";
import { renderedCodeRange } from "../../viewer/code/rendererDom";

const GROUP = "Project pane: files";

const { console: console_, web, api, sessions } = SAMPLE;
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });

/** A block of unrelated code, which a long file is padded with so that what it matches lies mid-file. */
const filler = (name: string) => `export function ${name}(documents: Document[]): Document[] {
  const seen = new Set<string>();
  return documents.filter((document) => {
    if (seen.has(document.id)) return false;
    seen.add(document.id);
    return true;
  });
}
`;

const LONG_DOCUMENT_FIELDS = Array.from({ length: 12 }, (_, i) => `field${i + 1}`).join(", ");

/** The search index: a long file whose mentions of "reindex" start past its first screen, two of them on one line, and
 * two lines long enough for the daemon to clip (the first is cut at its end, the second at its start). */
const INDEXER = [
  "import type { Document } from \"../src/documents\";",
  "",
  "/** The index the service answers queries from. */",
  ...["dedupe", "normalise", "tokenise", "stem", "rank", "group", "paginate"].flatMap((name) => filler(name).split("\n")),
  "/** Rebuilds the index from every document: a reindex runs in the background and swaps the new index in once it is",
  " * whole, so queries never see a half-built one. Called by the nightly reindex job and after a schema change. */",
  "export async function reindex(documents: Document[], options: ReindexOptions = {}): Promise<Index> {",
  "  const next = createIndex(options);",
  "  for (const document of dedupe(documents)) {",
  "    await next.add(document);",
  "  }",
  "  // Keep the old index until the reindex has finished, then replace it in one step.",
  "  return swap(next);",
  "}",
  "",
  `// Fields whose change makes a document be reindexed: ${LONG_DOCUMENT_FIELDS}, and every field added to the schema since this list was written, which is why the list is checked against the schema when the service starts.`,
  "",
  `// A document is skipped when ${Array.from({ length: 8 }, (_, i) => `condition${i + 1}`).join(", ")}, ${LONG_DOCUMENT_FIELDS} and every other field are the same as when it was last indexed, so nothing about it needs a reindex`,
  "",
  ...["merge", "split", "compact", "verify"].flatMap((name) => filler(name).split("\n")),
].join("\n");

const contentFiles: FixtureFiles = {
  ...SAMPLE_FILES,
  lib: { dir: true },
  "lib/indexer.ts": { text: INDEXER },
  scripts: { dir: true },
  "scripts/reindex.sh": { text: "#!/bin/sh\nset -eu\n\n# Rebuilds the search index from scratch.\nexec node ./dist/main.js reindex --all\n" },
  "src/jobs": { dir: true },
  "src/jobs/reindex.ts": { text: "import { reindex } from \"../../lib/indexer\";\nimport { loadDocuments } from \"../documents\";\n\n/** The nightly job. */\nexport async function nightlyReindex() {\n  await reindex(await loadDocuments(), { background: true });\n}\n" },
  "src/jobs/schedule.ts": { text: "export const schedule = {\n  // Runs the reindex job when traffic is lowest.\n  nightlyReindex: \"0 3 * * *\",\n};\n" },
};

/** Shows the session's project in the Files panel, once its header's Find in files button is there. */
const openFiles = [(ui: Ui) => ui.press(ui.session("Fix the summary layout")), (ui: Ui) => ui.showPanel("files"), (ui: Ui) => ui.waitForControl(ui.t("contentSearch.title"))];
const openModal = (ui: Ui) => ui.press(ui.t("contentSearch.title"));

/** Whether the search has ended with its results listed, and the selected one's file drawn in the preview where the
 * modal has room for one (a modal that fills the screen holds the list alone, with no split). */
const resultsShown = (doc: Document): boolean => {
  if (doc.querySelector("[data-content-search-result]") === null || doc.querySelector("[data-content-search-spinner]") !== null) return false;
  if (doc.querySelector("[data-content-search-list]") === null) return true;
  const frame = doc.querySelector<HTMLElement>("[data-content-search-preview] [role=region][tabindex]");
  return frame !== null && Boolean(renderedCodeRange(frame));
};
const waitForResults = (ui: Ui) => ui.waitUntil(resultsShown, "the results, with the selected one's file in the preview");

/** Opens Find in files from the Files header and types `query`, then waits for what the search ends in (`settled`):
 * by default its results. */
const searchFor = (query: string, { between = [], settled = waitForResults }: { between?: ((ui: Ui) => Promise<void>)[]; settled?: (ui: Ui) => Promise<void> } = {}) => [
  ...openFiles,
  openModal,
  ...between,
  (ui: Ui) => ui.type(query),
  settled,
];

export const projectContentSearchScenarios: Scenario[] = [
  {
    id: "files-content-search",
    group: GROUP,
    category: "normal",
    title: "Find in files with its results and the file preview",
    description: "The modal opened from the Files header, with \"reindex\" searched. Under the title, one line of small text: the scope \"In Website\" (the project named as a chip) at its start and the count (\"N matches in M files\", no incomplete mark) at its end. Below it, the query field with the Match case, Whole word and Use regular expression toggles at its end, then the File mask field and the Include ignored files toggle, with the same room above the fields and below them. The list holds one row per matching line, the matches marked as the viewer's current find match is, with the file name and line number at the row's end; one line is long, so its text is cut with an ellipsis where the daemon cut it. The first result is selected and, in the preview under the list, its file is shown read-only with line numbers, headed by the file name and its folder, scrolled to the result's line with its match marked. Arrow keys move the selection and the preview follows. Narrow the window with the width control to see the modal fill the screen and hold the list alone, with no preview.",
    state,
    files: contentFiles,
    steps: searchFor("reindex"),
  },
  {
    id: "files-content-search-folder",
    group: GROUP,
    category: "normal",
    title: "Find in files limited to a folder",
    description: "Find in folder, chosen on the src folder's menu, opens the modal with the scope line \"In Website › src\" under the title, the project and the folder each named as a chip and joined by a chevron icon; only the matches under src are listed, none of lib's or scripts'. The modal offers no way to widen the search.",
    state,
    files: contentFiles,
    steps: [
      ...openFiles,
      (ui) => ui.contextMenu("src"),
      (ui) => ui.press(ui.t("contentSearch.findInDirectory")),
      (ui) => ui.type("reindex"),
      waitForResults,
    ],
  },
  {
    id: "files-content-search-searching",
    group: GROUP,
    category: "normal",
    title: "Find in files while the search is running",
    description: "The search has started and nothing has come back: the list area holds the loading state \"Searching file contents…\" with its spinner, the one loading indicator, and the scope line under the title has no count at its end before the first result.",
    state,
    files: contentFiles,
    contentSearch: "searching",
    steps: searchFor("reindex", { settled: (ui) => ui.waitForText(ui.t("contentSearch.searching")) }),
  },
  {
    id: "files-content-search-empty",
    group: GROUP,
    category: "normal",
    title: "Find in files with no matches",
    description: "The search ended with nothing found: the list area holds the no-matches empty state, and the scope line under the title has no count at its end.",
    state,
    files: contentFiles,
    steps: searchFor("no-such-text", { settled: (ui) => ui.waitForText(ui.t("contentSearch.noMatch")) }),
  },
  {
    id: "files-content-search-incomplete-empty",
    group: GROUP,
    category: "exceptional",
    title: "Find in files with no matches from a search that is not whole",
    description: "The search stopped before it was whole and found nothing: the list area holds the no-matches empty state with a line under its heading saying not every file was searched, and the end of the scope line under the title shows \"0 matches in 0 files\" followed by the warning triangle named \"The search is incomplete\", whose popover says the search took too long and stopped.",
    state,
    files: contentFiles,
    contentSearch: { stoppedBy: "time" },
    steps: searchFor("no-such-text", { settled: (ui) => ui.waitForText(ui.t("contentSearch.noMatch.incomplete.description")) }),
  },
  {
    id: "files-viewer-at-line",
    group: "File viewer",
    category: "normal",
    title: "A file opened at a search result's line",
    description: "Enter on a result of Find in files closes the modal and opens the file in the viewer scrolled to the result's line, which sits mid-file, with the line's matches marked: the line number and the matched text are tinted as the viewer's current find match is. The remaining lines are plain.",
    state,
    files: contentFiles,
    steps: [...searchFor("reindex"), (ui) => ui.key("Enter"), (ui) => ui.codeDrawn(ui.t("viewer.contents", { name: "indexer.ts" }))],
  },
  {
    id: "files-content-search-incomplete",
    group: GROUP,
    category: "exceptional",
    title: "Find in files from a search that is not whole",
    description: "The matches are listed with the preview as usual, and a warning triangle named \"The search is incomplete\" follows the count at the end of the scope line under the title. The mouse rests on it: its popover opens above it, saying the search took too long and stopped, so not every match may be listed. A search stopped by another bound, or one that could not read some files, says so in its own words.",
    state,
    files: contentFiles,
    contentSearch: { stoppedBy: "time" },
    steps: [...searchFor("reindex"), (ui) => ui.hoverStatusMark(ui.t("contentSearch.partial"))],
  },
  {
    id: "files-content-search-invalid-pattern",
    group: GROUP,
    category: "exceptional",
    title: "Find in files with a pattern that cannot be compiled",
    description: "Use regular expression is on and the pattern \"reindex(\" has an unclosed group: the query field is outlined in the danger colour and, right under it as the field's error, a line says in the danger colour, in one sentence, that the regular expression is not valid because the parenthesis at character 8 is never closed (none of the regex engine's own text); the file mask beside it stays where it is, the scope line under the title has no count, and the list area is empty.",
    state,
    files: contentFiles,
    steps: searchFor("reindex(", {
      between: [(ui) => ui.press(ui.t("viewer.find.regex"))],
      settled: (ui) => ui.waitUntil((doc) => doc.querySelector('[aria-invalid="true"]') !== null, "the query marked invalid"),
    }),
  },
  {
    id: "files-content-search-invalid-mask",
    group: GROUP,
    category: "exceptional",
    title: "Find in files with a file mask that cannot be compiled",
    description: "The file mask \"*.ts, a[\" has an unclosed character class: the File mask field is outlined in the danger colour and, right under it and the Include ignored files toggle as the field's error, a line says in the danger colour, in one sentence, that the file mask \"a[\" (set apart as a reference) is not valid because the bracket is never closed; the query field keeps its normal outline and its place, the scope line under the title has no count, and the list area is empty.",
    state,
    files: contentFiles,
    steps: [
      ...searchFor("reindex"),
      (ui) => ui.focus(ui.t("contentSearch.fileMask")),
      (ui) => ui.type("*.ts, a["),
      (ui) => ui.waitUntil((doc) => doc.querySelector('[aria-invalid="true"]') !== null, "the file mask marked invalid"),
    ],
  },
  {
    id: "files-content-search-failed",
    group: GROUP,
    category: "exceptional",
    title: "Find in files with a search that cannot be run",
    description: "The search failed with the project's own folder gone: the list area holds the failure \"Could not search file contents\" with its explanation naming the project's folder (~/code/website) as a chip, the system's reason on a surface of its own and Try again below. The fields above stay in place.",
    state,
    searchError: sourceUnavailable("", "missing", "No such file or directory (os error 2)"),
    steps: searchFor("reindex", { settled: (ui) => ui.waitForControl(ui.t("error.tryAgain")) }),
  },
];
