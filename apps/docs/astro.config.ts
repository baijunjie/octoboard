import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";
import { docsTitles } from "./siteTitle.ts";
import { locales } from "./locales.ts";
import { retiredSegmentRedirects } from "./redirects.ts";
import { sidebar } from "./sidebar.ts";
import site from "./site.config.ts";

// The docs are deployed beside the website, so they use its icon rather than a copy (`apps/web/public/favicon.png`).
const siteIcon = `${site.origin}${site.websiteBase}favicon.png`;

export default defineConfig({
  site: site.origin,
  base: site.base,
  trailingSlash: "always",
  integrations: [
    starlight({
      // One title per language, keyed by its `lang`, worded as that language says it.
      title: Object.fromEntries(Object.entries(docsTitles).map(([language, title]) => [language, title.replace("{name}", site.productName)])),
      // The website's own icon. Starlight puts its base path in front of any address that is not a full URL.
      favicon: siteIcon,
      customCss: ["./src/styles/theme.css"],
      defaultLocale: "root",
      locales,
      social: [{ icon: "github", label: "GitHub", href: site.repositoryUrl }],
      sidebar,
      // The title is drawn with the website's logo and pixel wordmark.
      components: { SiteTitle: "./src/components/SiteTitle.astro" },
      routeMiddleware: "./src/routeData.ts",
    }),
    retiredSegmentRedirects(site.base, `${site.websiteBase}favicon.png`),
  ],
});
