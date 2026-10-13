import type {
  AccountFallback,
  ImportChoiceKind,
  ImportedConsole,
  InspectedConsole,
  InspectedProject,
  ProjectResolution,
} from "../protocol";

/** The choices a project's offers can name: the three directories the daemon works out. The fourth,
 * a directory the user points at, is never offered, and leaving the project out is no directory. */
export type OfferedChoice = Exclude<ImportChoiceKind, "directory">;

const OFFERED_CHOICES: readonly OfferedChoice[] = ["exported_path", "default_clone_dir", "clone"];

/** What the review holds for one project: which way it is placed and, for `directory`, the path the
 * user typed. `skip` leaves the project out of the import. */
export interface RowState {
  choice: OfferedChoice | "directory" | "skip";
  /** What the field handed the form, which is already trimmed. */
  directory: string;
}

/** The review's state, indexed as the document is: `[console][project]`, which is also how a
 * resolution names a project. */
export type ReviewState = RowState[][];

/** The choices a project's state allows, in the order the review lists them. Pointing at a
 * directory and leaving the project out are open to every project. */
export function choicesFor(project: InspectedProject): RowState["choice"][] {
  return [...OFFERED_CHOICES.filter((choice) => project.offers[choice] !== null), "directory", "skip"];
}

/** Every project on the choice the daemon preselected, and left out where it offered nothing. */
export function initialReview(consoles: InspectedConsole[]): ReviewState {
  return consoles.map((console) =>
    console.projects.map((project) => ({ choice: project.default ?? "skip", directory: "" })),
  );
}

/** Replaces one project's row; every other row is the same object as before. */
export function withRow(review: ReviewState, console: number, project: number, row: RowState): ReviewState {
  return review.map((rows, c) => (c === console ? rows.map((existing, p) => (p === project ? row : existing)) : rows));
}

/** How many projects `choice` can be applied to: those whose state offers it, and every project for
 * leaving it out. */
export function availableFor(consoles: InspectedConsole[], choice: OfferedChoice | "skip"): number {
  return consoles.reduce(
    (count, console) =>
      count + console.projects.filter((project) => choice === "skip" || project.offers[choice] !== null).length,
    0,
  );
}

/** `choice` on every project it is available for; a project it is not available for keeps its
 * row. */
export function applyToAll(
  consoles: InspectedConsole[],
  review: ReviewState,
  choice: OfferedChoice | "skip",
): ReviewState {
  return review.map((rows, c) =>
    rows.map((row, p) => {
      const project = consoles[c].projects[p];
      return choice === "skip" || project.offers[choice] !== null ? { ...row, choice } : row;
    }),
  );
}

/** Whether `path` is one the daemon takes as a directory the user pointed at: absolute, or starting
 * with `~/`, which it expands. Only the shape is judged. */
export function isUsableDirectory(path: string): boolean {
  return path.startsWith("/") || path.startsWith("~/");
}

/** What is wrong with the directory typed for a project that is placed in one, if anything. */
export function directoryProblem(row: RowState): "empty" | "not_absolute" | undefined {
  if (row.choice !== "directory") return undefined;
  const path = row.directory;
  if (path === "") return "empty";
  return isUsableDirectory(path) ? undefined : "not_absolute";
}

/** Whether any project's row still has a directory the daemon would refuse. */
export function hasDirectoryProblem(review: ReviewState): boolean {
  return review.some((rows) => rows.some((row) => directoryProblem(row) !== undefined));
}

/** The directory a row would place its project in, for the review to show before anything is
 * applied; none for a project left out and for a directory not typed yet. */
export function targetOf(project: InspectedProject, row: RowState): string | undefined {
  switch (row.choice) {
    case "skip":
      return undefined;
    case "directory":
      return row.directory || undefined;
    default:
      return project.offers[row.choice] ?? undefined;
  }
}

/** The resolutions to send: one per project that is placed, none for a project left out. */
export function resolutionsOf(review: ReviewState): ProjectResolution[] {
  return review.flatMap((rows, console) =>
    rows.flatMap((row, project): ProjectResolution[] => {
      if (row.choice === "skip") return [];
      return [
        {
          console,
          project,
          choice: row.choice === "directory" ? { kind: "directory", path: row.directory } : { kind: row.choice },
        },
      ];
    }),
  );
}

/** An account reference that fell back to its agent's default account, with whose it was. */
export interface PlacedFallback extends AccountFallback {
  console: string;
  /** The project holding the reference; absent when the console holds it. */
  project?: string;
}

/** What an import came to, counted and collected from the daemon's result. */
export interface ImportOutcome {
  consolesCreated: number;
  consolesFailed: number;
  projectsCreated: number;
  projectsLeftOut: number;
  projectsFailed: number;
  projectsNotAttempted: number;
  fallbacks: PlacedFallback[];
}

export function outcomeOf(consoles: ImportedConsole[]): ImportOutcome {
  const outcome: ImportOutcome = {
    consolesCreated: 0,
    consolesFailed: 0,
    projectsCreated: 0,
    projectsLeftOut: 0,
    projectsFailed: 0,
    projectsNotAttempted: 0,
    fallbacks: [],
  };
  for (const console of consoles) {
    if (console.status === "created") outcome.consolesCreated += 1;
    else outcome.consolesFailed += 1;
    outcome.fallbacks.push(...console.account_fallbacks.map((fallback) => ({ ...fallback, console: console.name })));
    for (const project of console.projects) {
      if (project.status === "created") outcome.projectsCreated += 1;
      else if (project.status === "left_out") outcome.projectsLeftOut += 1;
      else if (project.status === "failed") outcome.projectsFailed += 1;
      else outcome.projectsNotAttempted += 1;
      outcome.fallbacks.push(
        ...project.account_fallbacks.map((fallback) => ({ ...fallback, console: console.name, project: project.name })),
      );
    }
  }
  return outcome;
}
