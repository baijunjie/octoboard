import { useMemo, type Ref } from "react";
import { mergeRefs } from "react-aria";

import { useListMotion } from "../components/useListMotion";

/** Preserve the browser's scroll/focus ref while tracking only its rendered virtual rows. */
export function useBrowserListMotion(forwarded: Ref<HTMLDivElement>): Ref<HTMLDivElement> {
  const motion = useListMotion<HTMLDivElement>({ virtualized: true });
  return useMemo(() => mergeRefs(forwarded, motion), [forwarded, motion]);
}
