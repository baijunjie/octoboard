import type { PlainMessageKey } from "./i18n/catalog";
import type { WorktreeDelivery, WorktreeOverride, WorktreeOverrideChanges, WorktreeSettings } from "./protocol";

/** The worktree settings of a fresh installation, as the daemon starts them (see "The four settings" in
 * `docs/product/worktree-settings.md`). Stands in for the daemon's values before it has sent any, and in states built
 * without them. */
export const DEFAULT_WORKTREE_SETTINGS: WorktreeSettings = {
  use_worktrees: true,
  rebase_before_merge: true,
  delivery: "ask",
  revert_gate: false,
};

/** A project that holds none of its own worktree settings: every item follows the application-wide value. */
export const FOLLOWING_WORKTREE_SETTINGS: WorktreeOverride = {
  use_worktrees: null,
  rebase_before_merge: null,
  delivery: null,
  revert_gate: null,
};

/** The delivery choices in the order they are offered, each with the message that words it. */
export const WORKTREE_DELIVERY_CHOICES: { value: WorktreeDelivery; label: PlainMessageKey }[] = [
  { value: "ask", label: "settings.worktrees.delivery.ask" },
  { value: "merge", label: "settings.worktrees.delivery.merge" },
  { value: "pull_request", label: "settings.worktrees.delivery.pullRequest" },
];

const ITEMS = ["use_worktrees", "rebase_before_merge", "delivery", "revert_gate"] as const;

/** The items of `current` that differ from `initial`, as an `update_project` change: `null` puts an item back to
 * following the application-wide value, a value makes it the project's own, and an item that did not change is left out. */
export function changedWorktreeItems(current: WorktreeOverride, initial: WorktreeOverride): WorktreeOverrideChanges {
  return Object.fromEntries(ITEMS.filter((item) => current[item] !== initial[item]).map((item) => [item, current[item]]));
}
