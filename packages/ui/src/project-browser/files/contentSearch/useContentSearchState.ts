import { useState } from "react";

import { DEFAULT_CONTENT_SEARCH_OPTIONS, type ContentSearchOptions } from "./useContentSearch";

/** Whether Find in files is open, and on what: the whole project, or the folder it was opened on (a wire path). */
export interface ContentSearchOpening {
  directory?: string;
}

/** The state of a project's Find in files: whether the modal is open and on what (`show`, `close`), and the query and
 * options, which outlive the modal so that opening it again shows the last search. */
export function useContentSearchState() {
  const [open, setOpen] = useState<ContentSearchOpening>();
  const [query, setQuery] = useState("");
  const [options, setOptions] = useState<ContentSearchOptions>(DEFAULT_CONTENT_SEARCH_OPTIONS);
  return {
    open,
    query,
    options,
    setQuery,
    setOptions,
    /** Opens the modal on the whole project, or limited to `directory`. */
    show: (directory?: string) => setOpen(directory === undefined ? {} : { directory }),
    close: () => setOpen(undefined),
  };
}
