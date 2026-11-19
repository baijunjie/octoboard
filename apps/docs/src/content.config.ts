import { docsLoader } from "@astrojs/starlight/loaders";
import { docsSchema } from "@astrojs/starlight/schema";
import { defineCollection } from "astro:content";
import { locales } from "../locales.ts";

const localeDirectories = new Map(
  Object.keys(locales)
    .filter((key) => key !== "root")
    .map((key) => [key.toLowerCase(), key]),
);

// A page's id is its path, lower-cased with spaces as hyphens, except for a leading locale directory, which keeps the
// locale's own spelling: lower-casing it would turn `zh-TW/` into `zh-tw/`, which matches no locale (nor does
// `zh-cn/`), so the Chinese pages would be taken for English ones at a lower-cased path.
function pageId({ entry, data }: { entry: string; data: Record<string, unknown> }): string {
  if (typeof data.slug === "string") return data.slug;
  const segments = entry
    .replace(/\.[^./]+$/, "")
    .split("/")
    .map((segment) => segment.toLowerCase().replace(/\s+/g, "-"));
  if (segments.length > 1 && segments.at(-1) === "index") segments.pop();
  const locale = localeDirectories.get(segments[0]!);
  if (locale) segments[0] = locale;
  return segments.join("/");
}

export const collections = {
  docs: defineCollection({ loader: docsLoader({ generateId: pageId }), schema: docsSchema() }),
};
