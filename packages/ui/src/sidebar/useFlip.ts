import { useListMotion } from "../components/motion/useListMotion";

/** Compatibility for sidebar rows marked with data-flip. */
export function useFlip<T extends HTMLElement>(): React.RefObject<T | null> {
  return useListMotion<T>();
}
