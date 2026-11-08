import type { PlainMessageKey } from "../i18n/catalog";
import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";

/** Whether the desktop app keeps the computer and its display awake (Prevent sleep): the Settings switch the desktop
 * app offers. Off until the user turns it on; `useKeepAwakeSync` hands the shell whether to hold. */
export const keepAwake = createPersistedPreference<boolean>(
  PREFERENCE_KEYS.keepAwake,
  (raw) => raw === "true",
  (value) => String(value),
);

/** When Prevent sleep holds: whenever the app is running, or only while a session is working. */
export const KEEP_AWAKE_WHEN = ["always", "working"] as const;
export type KeepAwakeWhen = (typeof KEEP_AWAKE_WHEN)[number];

/** The label of each choice of when, the same in the Settings row and in an import's cards. */
export const KEEP_AWAKE_WHEN_LABEL = { always: "settings.keepAwake.when.always", working: "settings.keepAwake.when.working" } as const satisfies Record<KeepAwakeWhen, PlainMessageKey>;

export const isKeepAwakeWhen = (value: unknown): value is KeepAwakeWhen => KEEP_AWAKE_WHEN.includes(value as KeepAwakeWhen);

/** The choice of when, once Prevent sleep is on: while sessions are working until the user picks otherwise. */
export const keepAwakeWhen = createPersistedPreference<KeepAwakeWhen>(
  PREFERENCE_KEYS.keepAwakeWhen,
  (raw) => (isKeepAwakeWhen(raw) ? raw : "working"),
  (value) => value,
);

/** Whether the computer is to be held awake: Prevent sleep is on, and either it holds always or a session
 * is working. */
export function shouldKeepAwake(enabled: boolean, when: KeepAwakeWhen, sessionWorking: boolean): boolean {
  return enabled && (when === "always" || sessionWorking);
}
