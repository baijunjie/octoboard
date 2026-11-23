import type { errorAlerts } from "./errorAlerts";

/** The error-alert queue once a test file has loaded it, for the shared test setup to empty after every test
 * (`src/test/alertQueue.ts`) without loading the queue itself, which a module with no imports allows. It holds the
 * queue of this module's instance: a queue loaded after `vi.resetModules()` enters the fresh instance instead. */
export const errorAlertsUnderTest = new Set<typeof errorAlerts>();
