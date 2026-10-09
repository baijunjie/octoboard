// Stands in, in a build, for the two modules that list every bundled highlighting theme behind its
// own dynamic import — `@pierre/theming/themes` and Shiki's internal `themes.mjs` (redirected by
// `vite.config.ts`) — which would otherwise put all of them, about 1.8 MB of chunks, in the
// bundle although the viewer uses two. It offers only those two, under both modules' exports and
// in their shapes, so the renderer library and Shiki resolve them as before. A theme name missing
// here fails to load, which the viewer shows as plain text.
import { createThemeCollection } from "@pierre/theming";
import { normalizeTheme, type ThemeRegistrationAny } from "shiki/core";

const loadLight = () => import("@shikijs/themes/github-light-high-contrast");
const loadDark = () => import("@shikijs/themes/github-dark-high-contrast");

type Loaded = ThemeRegistrationAny | { default: ThemeRegistrationAny };

interface ThemeOptions {
  name: string;
  load: () => Promise<Loaded>;
  colorScheme?: "light" | "dark";
  collection?: string;
  displayName?: string;
}

/** What `@pierre/theming/themes` exports under this name: a descriptor whose loader normalizes the
 * theme it loads. */
export function createTheme({ name, load, colorScheme, collection, displayName }: ThemeOptions) {
  return {
    name,
    colorScheme,
    collection,
    displayName,
    load: async () => {
      const loaded = await load();
      return normalizeTheme("default" in loaded ? loaded.default : loaded);
    },
  };
}

export const shikiThemes = createThemeCollection({
  themes: [
    createTheme({
      name: "github-light-high-contrast",
      collection: "shiki",
      colorScheme: "light",
      load: loadLight,
    }),
    createTheme({
      name: "github-dark-high-contrast",
      collection: "shiki",
      colorScheme: "dark",
      load: loadDark,
    }),
  ],
});

// `@pierre/theming/themes`'s exports.
export const pierreThemes = createThemeCollection({ themes: [] });

export const themes = createThemeCollection({ themes: [pierreThemes, shikiThemes] });

// Shiki's `themes.mjs` exports.
export const bundledThemesInfo = [
  { id: "github-light-high-contrast", displayName: "GitHub Light High Contrast", type: "light", import: loadLight },
  { id: "github-dark-high-contrast", displayName: "GitHub Dark High Contrast", type: "dark", import: loadDark },
] as const;

export const bundledThemes = Object.fromEntries(bundledThemesInfo.map((info) => [info.id, info.import]));
