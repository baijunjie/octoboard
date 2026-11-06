import { useTheme as useHeroTheme } from "@heroui/react";

/** The theme HeroUI applies to `<html>` (its class and `data-theme`), as a hook that holds its own `localStorage`-backed
 * state: `theme` is the stored choice ("light", "dark" or "system", which follows the OS), `resolvedTheme` what it
 * resolves to now (undefined only without a DOM), and `setTheme` stores a new choice. One instance at most may be
 * mounted, or each tracks its own copy of the choice. */
export function useTheme(): { theme: string; resolvedTheme: string | undefined; setTheme: (theme: string) => void } {
  const { theme, resolvedTheme, setTheme } = useHeroTheme();
  return { theme, resolvedTheme, setTheme };
}
