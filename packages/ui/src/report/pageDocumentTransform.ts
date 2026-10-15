import { createContext } from "react";

/** A trusted host may extend the completed document before its first navigation. Keeping the
 * transform in composition avoids replacing a mounted sandbox's document while it is loading. */
export const PageDocumentTransform = createContext<(srcdoc: string) => string>(
  (srcdoc) => srcdoc,
);
