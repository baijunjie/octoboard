// The files behind the path links of the terminal scenarios: what the fixture daemon reads when a
// link opens in the file viewer, in the project's folder and in a worktree's.
import type { Fixture } from "../../fixtures/fixture";
import type { FixtureFiles } from "../../fixtures/sampleFiles";
import { IMAGES } from "../../fixtures/viewerImages";

const SUMMARY = `import type { ReactElement } from "react";

import { formatCount } from "./format";
import { useSummary, type SummaryRow } from "./useSummary";

/** How many rows the summary shows before it folds the rest. */
const VISIBLE_ROWS = 5;

interface SummaryProps {
  session: string;
  compact?: boolean;
}

function rowLabel(row: SummaryRow): string {
  return \`\${row.title} (\${formatCount(row.count)})\`;
}

function foldedCount(rows: SummaryRow[]): number {
  return Math.max(0, rows.length - VISIBLE_ROWS);
}

export function Summary({ session, compact = false }: SummaryProps): ReactElement {
  const { rows, loading } = useSummary(session);
  const shown = rows.slice(0, VISIBLE_ROWS);
  const folded = foldedCount(rows);

  if (loading) {
    return <p className="summary summary--loading">Loading the summary</p>;
  }

  return (
    <section className={compact ? "summary summary--compact" : "summary"}>
      <h2 className="summary__title">Summary</h2>
      <ul className="summary__rows">
        {shown.map((row) => (
          <li key={row.id} className="summary__row">
            {rowLabel(row)}
          </li>
        ))}
      </ul>
      {folded > 0 && (
        <p className="summary__folded">
          {formatCount(folded)} more
        </p>
      )}
    </section>
  );
}
`;

const SUMMARY_NOTES = `# Summary layout notes

How the summary behaves at the widths the window can take.

## Widths

- Wide: the rows sit beside the title.
- Narrow: the rows stack under the title.
- Compact: the title and the first row share a line.

## Open questions

The folded count has to stay visible when the window is narrower than 480px.

Whether the compact summary keeps its title is left to the design review.

## Decided

- The summary never scrolls on its own.
- Long titles wrap rather than truncate.
`;

const PLAN_README = `# Summary layout plan

Rework how the summary lays out its rows at each window width.

## Milestones

1. Stack the rows under the title in a narrow window.
2. Keep the folded count visible below 480px.
3. Share a line between the title and the first row in the compact summary.

## Status

Milestone 1 is done; the others are open.
`;

/** The project folder's files: a component long enough to scroll to line 42, a document with a
 * heading above the line a link names, a stylesheet, and a plan folder with a README. */
export const PATH_LINK_FILES: FixtureFiles = {
  "": { dir: true },
  src: { dir: true },
  "src/summary": { dir: true },
  "src/summary/Summary.tsx": { text: SUMMARY },
  "src/summary/Summary.css": { text: `.summary {
  width: 100%;
  max-width: 480px;
}

.summary--compact .summary__title {
  display: inline;
}
` },
  bin: { dir: true },
  "bin/helper": { binary: { size: 48_213 } },
  docs: { dir: true },
  "docs/summary-notes.md": { text: SUMMARY_NOTES },
  "docs/plans": { dir: true },
  "docs/plans/summary-layout": { dir: true },
  "docs/plans/summary-layout/README.md": { text: PLAN_README },
};

/** What the side worktree holds: a file the project's own folder does not have. */
export const PATH_LINK_WORKTREE_FILES: FixtureFiles = {
  "": { dir: true },
  src: { dir: true },
  "src/ui.ts": { text: `// The experimental layout, in the side worktree.
import { Summary } from "./summary/Summary";

export const layout = {
  summary: Summary,
  columns: 3,
  gap: 12,
};
` },
};

/** The temporary folder's file the sample output names: a screenshot the agent wrote, where `read_path_link` reads it. */
export const TEMP_IMAGE_PATH = "/private/tmp/octoboard/scratchpad/icon/sheet3.png";

/** Which of the paths the sample output names the fixture daemon finds, and where it reads each from: files of the
 * project, one in a side worktree, a program the system could launch (found, but only the viewer shows it), a folder
 * with a README, and an image in a temporary folder. */
export const PATH_LINKS = {
  "/Users/dev/code/website/src/summary/Summary.css": { view: { path: "src/summary/Summary.css" } },
  "/Users/dev/code/website/src/summary/Summary.tsx": { view: { path: "src/summary/Summary.tsx" } },
  "/Users/dev/code/website/docs/summary-notes.md": { view: { path: "docs/summary-notes.md" } },
  "/Users/dev/code/website/bin/helper": { kind: "file", refused: "launchable", view: { path: "bin/helper" } },
  "/Users/dev/code/website/docs/plans/summary-layout": { kind: "directory", view: { path: "docs/plans/summary-layout/README.md" } },
  "/Users/dev/code/search-api-experiments/src/ui.ts": { view: { worktree: "wt-side", path: "src/ui.ts" } },
  [TEMP_IMAGE_PATH]: { content: { image: IMAGES.after_png } },
} satisfies NonNullable<Fixture["pathLinks"]>;

/** What the path link scenarios share: the sample output naming files, the project's and the side worktree's files, and
 * which of the paths the fixture daemon finds. */
export const PATH_LINK_FIXTURE = {
  terminal: "paths",
  files: PATH_LINK_FILES,
  worktreeFiles: { "wt-side": PATH_LINK_WORKTREE_FILES },
  pathLinks: PATH_LINKS,
} satisfies Fixture;
