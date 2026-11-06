import React, { createContext, useContext } from "react";

/** The names the wrappers give the controls the third-party components render themselves, in the application's
 * language. The layer imports nothing of the application, so the application provides them (`PrimitiveLabelsProvider`,
 * from its language provider); without one, the library's own English names stay. */
export interface PrimitiveLabels {
  /** A tab strip's scroll buttons, by the way each scrolls. */
  scrollTabs: { up: string; down: string; left: string; right: string };
}

const PrimitiveLabelsContext = createContext<PrimitiveLabels | undefined>(undefined);

export function PrimitiveLabelsProvider({ labels, children }: { labels: PrimitiveLabels; children: React.ReactNode }): React.ReactElement {
  return <PrimitiveLabelsContext.Provider value={labels}>{children}</PrimitiveLabelsContext.Provider>;
}

/** The labels the application provides, if it does. */
export function usePrimitiveLabels(): PrimitiveLabels | undefined {
  return useContext(PrimitiveLabelsContext);
}
