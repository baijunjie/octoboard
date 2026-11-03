import React, { createContext, useCallback, useContext, useId, useLayoutEffect, useState } from "react";

/** Hands a mark over to be drawn beside the link it was found in; returns what takes it back. */
type PlaceBesideLink = (key: string, mark: React.ReactNode) => () => void;

const PlaceBesideLinkContext = createContext<PlaceBesideLink | null>(null);

/**
 * A link of the document, with the marks of its failed content drawn right after it. A failed
 * image or formula in a link is a mark that opens a popover, a button, and a button inside an
 * anchor is interactive content nested in interactive content: a press on it would also be a press
 * on the link. So the content hands its mark to `useMarkBesideLink` and the link draws it outside
 * the anchor instead.
 */
export function LinkWithMarks({ children }: { children: React.ReactNode }): React.ReactElement {
  const [marks, setMarks] = useState<ReadonlyMap<string, React.ReactNode>>(new Map());
  const place = useCallback<PlaceBesideLink>((key, mark) => {
    setMarks((current) => new Map(current).set(key, mark));
    return () =>
      setMarks((current) => {
        const next = new Map(current);
        next.delete(key);
        return next;
      });
  }, []);
  return (
    <PlaceBesideLinkContext value={place}>
      {children}
      {[...marks].map(([key, mark]) => <React.Fragment key={key}>{mark}</React.Fragment>)}
    </PlaceBesideLinkContext>
  );
}

/** Draws `mark` beside the link this is inside, when it is inside one, and returns whether it did,
 * in which case the caller draws nothing in its place. */
export function useMarkBesideLink(mark: React.ReactNode | undefined): boolean {
  const place = useContext(PlaceBesideLinkContext);
  const key = useId();
  const placed = place !== null && mark !== undefined;
  useLayoutEffect(() => (placed ? place(key, mark) : undefined), [placed, place, key, mark]);
  return placed;
}
