// The error queue outlives the component that reported an alert.
import { act } from "react";

import { errorAlertsUnderTest } from "../components/alerts/errorAlertsUnderTest";

/** Acknowledges every queued alert, including one without a mounted alert host. It reaches the queue the file has
 * loaded, the one its static imports share, and loads none where the file has not. */
export function acknowledgeAllAlerts(): void {
  for (const queue of errorAlertsUnderTest) {
    act(() => {
      while (queue.getSnapshot().length) queue.acknowledge();
    });
  }
}
