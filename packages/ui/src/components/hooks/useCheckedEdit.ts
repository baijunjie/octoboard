import { useState } from "react";

import { baseVersions, reachedConflict, writeConflict, type FieldVersions, type WriteConflict } from "../../errors/writeConflict";
import { useLatest } from "./useLatest";

/** What a form says about the record it edits, for `useCheckedEdit`. */
export interface CheckedForm {
  /** Whether the store holds the path's value as the form read it. */
  same: (path: string) => boolean;
  /** Whether the user changed the path's field from what the form read. */
  edited: (path: string) => boolean;
  /** Puts each path back to what the store holds now, as what the form read of it. It runs while the form renders, so it
   * only sets state. */
  reset: (paths: readonly string[]) => void;
  /** Whether the form's own write is in flight, whose outcome the dialog reports itself. */
  writing?: boolean;
}

/**
 * The checked-write side of a form that edits a record the daemon versions (a console, a project, an account): the
 * versions the form is based on, and what happens when the record moves past them.
 *
 * `opened` is the record's `versions` when the form read it, and `base(paths)` the `base_versions` of a write that
 * changes `paths`. A form sends only the fields the user changed, so that two clients changing different fields do not
 * conflict. `current` is the store's `versions` of the record. As soon as it shows a path at a version other than the
 * one the form is based on (another client, an agent or a single action changed the field), the hook bases every such
 * path on the new version. `form.same(path)` says whether the store holds the path's value as the form read it, and for
 * the paths it does not the hook calls `form.reset(paths)` once, which puts each back to what the record holds now,
 * whether or not the user had edited it, and leaves the user's other edits alone. A version that moved over the same
 * value (every one does after a daemon restart) is only re-based: the user's edit stays. A write the daemon refuses as a
 * `write_conflict` is the same case arriving late: the change reached the daemon before it reached this window, and the
 * reset follows once the store holds it (`refused` tells the hook which refusal). The error alert of a refused write is
 * raised by the dialog's action as for any other failure.
 *
 * `replaced` is what the form is to announce (see `ChangedElsewhereAnnouncer`): `count` grows each time a field the
 * user had edited (`form.edited`) was replaced, and `paths` are the fields of the latest together with those of the
 * earlier ones the announcer has not written yet, since a change that replaces one still waiting is the only one said.
 * The form calls `said` when the announcer has written. A field the user had not touched is replaced silently, as is
 * any made while the form's own write is in flight or that a refusal's alert speaks for.
 */
export function useCheckedEdit(
  opened: FieldVersions,
  current: FieldVersions | undefined,
  form: CheckedForm,
): {
  base: (paths: readonly string[]) => FieldVersions | undefined;
  refused: (error: unknown) => void;
  replaced: { count: number; paths: readonly string[] };
  said: () => void;
} {
  const [based, setBased] = useState(opened);
  // `written`: the announcer has said the paths, so the next change starts them afresh.
  const [replaced, setReplaced] = useState<{ count: number; paths: readonly string[]; written: boolean }>({ count: 0, paths: [], written: true });
  // The conflict a refusal named, until the store holds the versions it names.
  const [refusal, setRefusal] = useState<WriteConflict>();
  const latestCurrent = useLatest(current);
  const moved = Object.keys(based).filter((path) => current?.[path] !== undefined && current[path] !== based[path]);
  if (moved.length > 0 && current) {
    const differing = moved.filter((path) => !form.same(path));
    if (differing.length > 0) {
      const announced = differing.filter(form.edited);
      form.reset(differing);
      if (announced.length > 0 && !form.writing && !refusal) {
        const gathered = replaced.written ? announced : [...new Set([...replaced.paths, ...announced])];
        setReplaced({ count: replaced.count + 1, paths: gathered, written: false });
      }
    }
    setBased({ ...based, ...Object.fromEntries(moved.map((path) => [path, current[path]])) });
  }
  if (refusal && reachedConflict(refusal, current)) setRefusal(undefined);
  return {
    base: (paths) => baseVersions(based, paths),
    refused: (error) => {
      const named = writeConflict(error);
      if (named && !reachedConflict(named, latestCurrent.current)) setRefusal(named);
    },
    replaced: { count: replaced.count, paths: replaced.paths },
    said: () => setReplaced((prior) => ({ ...prior, written: true })),
  };
}
