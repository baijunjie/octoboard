// The error queue outlives the component that reported an alert.
import { act } from "react";

import { errorAlerts } from "../components/alerts/errorAlerts";

/** Acknowledges every queued alert, including one without a mounted alert host. */
export function acknowledgeAllAlerts(): void {
  act(() => {
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  });
}
