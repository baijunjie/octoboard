import { MAX_AUTO_RECONNECT_ATTEMPTS, reconnectDelay } from "./reconnectPolicy";

/**
 * Fake time that outlasts every automatic reconnect: the waits of attempts 0 up to and including the one past the budget,
 * summed, so a test that asserts nothing is retried has run the whole backoff schedule, and a retry that is still due
 * has fired.
 */
export const PAST_ANY_RETRY_MS = Array.from({ length: MAX_AUTO_RECONNECT_ATTEMPTS + 1 }, (_, attempt) => reconnectDelay(attempt)).reduce((sum, wait) => sum + wait, 0);
