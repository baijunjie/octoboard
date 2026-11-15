// Hands the window's daemon identity to the preferences, which import nothing of the platform layer. Imported first by
// the entry (`main.tsx`), before any module that creates a preference: each reads its stored value under its key as it
// is created, and a remote window's daemon-scoped preferences are kept under that daemon's identity.

import { daemonIdentity } from "./platform";
import { setPreferenceDaemonIdentity } from "./preferences/persistedPreference";

setPreferenceDaemonIdentity(daemonIdentity());
