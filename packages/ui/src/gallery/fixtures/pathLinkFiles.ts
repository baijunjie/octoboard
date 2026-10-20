// The files behind the path links of the terminal scenarios: what the fixture daemon reads when a
// link opens in the file viewer, in the project's folder and in a worktree's.
import type { FixtureFiles } from "./projectFiles";

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
