// The page a third-party licenses scenario renders in place of the app: the real `ThirdPartyLicensesDialog` that
// Settings > About opens, with the list it loads given by the scenario. About itself cannot be given a loader, and
// the build's own list differs between checkouts, so the dialog is mounted here directly.

import React, { useState } from "react";

import type { ThirdPartyLicenses } from "../about/thirdPartyLicenses";
import { ThirdPartyLicensesDialog } from "../about/ThirdPartyLicensesDialog";
import type { Scenario } from "./scenario";

type State = Exclude<Scenario["licensesDialog"], undefined>;

/** A second load that follows the first by less than this is the same mount's: React's development double effect. */
const SAME_MOUNT_MS = 250;

/** What the dialog's loader answers for the scenario's `licensesDialog`: the list, a load that never ends, a
 * rejected one, or one that rejects the first time and answers with the list after that (Try again). */
function loaderOf(state: State): () => Promise<ThirdPartyLicenses> {
  if (state === "loading") return () => new Promise<never>(() => {});
  if (state === "failed") return () => Promise.reject(new Error("Gallery: the list failed to load."));
  if ("failsOnce" in state) {
    let failedAt: number | undefined;
    return () => {
      const now = performance.now();
      if (failedAt === undefined || now - failedAt < SAME_MOUNT_MS) {
        failedAt ??= now;
        return Promise.reject(new Error("Gallery: the list failed to load."));
      }
      return Promise.resolve(state.failsOnce);
    };
  }
  return () => Promise.resolve(state);
}

export function LicensesGallery({ state }: { state: State }): React.ReactElement {
  // One loader for the page's life, so that "fails once" is counted across a Try again.
  const [load] = useState(() => loaderOf(state));
  return <ThirdPartyLicensesDialog load={load} onClose={() => {}} />;
}
