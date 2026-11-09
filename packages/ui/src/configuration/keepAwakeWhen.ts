import type { PlainMessageKey } from "../i18n/catalog";
import type { KeepAwakeWhen } from "../protocol";

/** When Prevent sleep holds (`Settings.keep_awake_when`): whenever the daemon is running, or only while a session is
 * working. */
export const KEEP_AWAKE_WHEN: readonly KeepAwakeWhen[] = ["always", "working"];

/** The label of each choice of when, the same in the Settings row and in an import's cards. */
export const KEEP_AWAKE_WHEN_LABEL = { always: "settings.keepAwake.when.always", working: "settings.keepAwake.when.working" } as const satisfies Record<KeepAwakeWhen, PlainMessageKey>;

/** Whether `value` names a choice of when; a settings reply or document carries it as text. */
export const isKeepAwakeWhen = (value: unknown): value is KeepAwakeWhen => KEEP_AWAKE_WHEN.includes(value as KeepAwakeWhen);
