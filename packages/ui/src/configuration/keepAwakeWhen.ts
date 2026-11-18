import type { PlainMessageKey } from "../i18n/catalog";
import type { KeepAwakeWhen } from "../protocol";
import { knownValues } from "../values/knownValue";

/** When Prevent sleep holds (`Settings.keep_awake_when`): whenever the daemon is running, or only while a session is
 * working. */
export const KEEP_AWAKE_WHEN: readonly KeepAwakeWhen[] = ["always", "working"];

/** The label of each choice of when, the same in the Settings row and in an import's cards. */
export const KEEP_AWAKE_WHEN_LABEL = { always: "settings.keepAwake.when.always", working: "settings.keepAwake.when.working" } as const satisfies Record<KeepAwakeWhen, PlainMessageKey>;

/** What the Settings row of when says under its name for each choice, which the Settings search finds it by. */
export const KEEP_AWAKE_WHEN_DESCRIPTION = { always: "settings.keepAwake.when.description.always", working: "settings.keepAwake.when.description.working" } as const satisfies Record<KeepAwakeWhen, PlainMessageKey>;

/** Whether `value` names a choice of when this build knows; a settings reply or document carries it as text, and a
 * newer daemon may send one this build does not, which the Settings row shows as an empty choice and nothing is
 * written for. */
export const isKeepAwakeWhen = knownValues<KeepAwakeWhen>({ always: true, working: true });
