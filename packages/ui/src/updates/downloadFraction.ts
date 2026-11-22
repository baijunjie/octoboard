import type { UpdateProgress } from "../platform";

/** How much of an update's download has arrived, from 0 to 1; `undefined` while there is no download or its total size
 * is not known, which a progress bar shows as indeterminate. */
export function downloadFraction(progress: UpdateProgress | undefined): number | undefined {
  return progress?.total ? Math.max(0, Math.min(progress.downloaded / progress.total, 1)) : undefined;
}
