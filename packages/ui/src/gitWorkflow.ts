import type { PlainMessageKey } from "./i18n/catalog";
import type { GitDelivery, GitDevelopment, GitWorkflowOverride, GitWorkflowOverrideChanges, GitWorkflowSettings } from "./protocol";

/** The git workflow settings of a fresh installation, as the daemon starts them (see "The five settings" in
 * `docs/product/git-workflow-settings.md`). Stands in for the daemon's values before it has sent any, and in states built
 * without them. */
export const DEFAULT_GIT_WORKFLOW: GitWorkflowSettings = {
  development: "worktree",
  squash_before_delivery: false,
  rebase_before_merge: true,
  delivery: "ask",
  revert_gate: false,
};

/** A project that holds none of its own git workflow settings: every item follows the application-wide value. */
export const FOLLOWING_GIT_WORKFLOW: GitWorkflowOverride = {
  development: null,
  squash_before_delivery: null,
  rebase_before_merge: null,
  delivery: null,
  revert_gate: null,
};

/** The development choices in the order they are offered, each with the message that words it and the one that
 * explains it. */
export const DEVELOPMENT_CHOICES: { value: GitDevelopment; label: PlainMessageKey; description: PlainMessageKey }[] = [
  {
    value: "in_place",
    label: "settings.gitWorkflow.development.inPlace",
    description: "settings.gitWorkflow.development.description.inPlaceWithPreparation",
  },
  {
    value: "branch",
    label: "settings.gitWorkflow.development.branch",
    description: "settings.gitWorkflow.development.description.branch",
  },
  {
    value: "worktree",
    label: "settings.gitWorkflow.development.worktree",
    description: "settings.gitWorkflow.development.description.worktree",
  },
];

/** The delivery choices in the order they are offered, each with the message that words it. */
export const DELIVERY_CHOICES: { value: GitDelivery; label: PlainMessageKey }[] = [
  { value: "ask", label: "settings.gitWorkflow.delivery.ask" },
  { value: "merge", label: "settings.gitWorkflow.delivery.merge" },
  { value: "pull_request", label: "settings.gitWorkflow.delivery.pullRequest" },
];

const ITEMS = ["development", "squash_before_delivery", "rebase_before_merge", "delivery", "revert_gate"] as const;

/** The items of `current` that differ from `initial`, as an `update_project` change: `null` puts an item back to
 * following the application-wide value, a value makes it the project's own, and an item that did not change is left out. */
export function changedGitWorkflowItems(current: GitWorkflowOverride, initial: GitWorkflowOverride): GitWorkflowOverrideChanges {
  return Object.fromEntries(ITEMS.filter((item) => current[item] !== initial[item]).map((item) => [item, current[item]]));
}
