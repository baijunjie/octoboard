import { useEffect, useState } from "react";

/** False while a view paints for the first time, true from the effect after that on. A failure shown
 * on the screen is announced only if it appears once this is true: one already there as the view
 * opens is part of what the view opened with (`ErrorAlert`'s `announce`). */
export function useHasPainted(): boolean {
  const [painted, setPainted] = useState(false);
  useEffect(() => setPainted(true), []);
  return painted;
}
