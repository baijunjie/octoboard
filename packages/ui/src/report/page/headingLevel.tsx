import { createContext, useContext } from "react";

/** The level of the heading nearest above a component: the page's title, a section's heading or a callout's title. What
 * a component draws as a heading of its own sits one level below it, and its Markdown's headings start there, so the
 * outline of a page stays in order. */
export const HeadingLevelContext = createContext(3);

/** The level below the nearest heading, no lower than `h6`, as the level and the tag, with `inner`: what the context
 * holds for the content under a heading drawn at that level, or the one it already holds where there is no heading. */
export function useChildHeadingLevel(drawn: boolean): { Tag: "h4" | "h5" | "h6"; inner: number } {
  const above = useContext(HeadingLevelContext);
  const level = Math.min(above + 1, 6) as 4 | 5 | 6;
  return { Tag: `h${level}`, inner: drawn ? level : above };
}
