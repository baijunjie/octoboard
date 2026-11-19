// Where the docs are served: `/docs/` under the website's own base path. The website's deployment settings are read
// here unchanged (`NUXT_PUBLIC_SITE_ORIGIN`, `NUXT_APP_BASE_URL`, defaulting to the `websiteUrl` of `config/app.json`
// and `/`, as `apps/web/site.config.ts` does), so one deployment configures both sites.
import appConfig from "../../config/app.json" with { type: "json" };

export const docsSegment = "docs";

const origin = new URL(process.env.NUXT_PUBLIC_SITE_ORIGIN || appConfig.websiteUrl).origin;
const websiteBase = `/${(process.env.NUXT_APP_BASE_URL || "/").replace(/^\/+|\/+$/g, "")}/`.replace("//", "/");

export default {
  origin,
  websiteBase,
  base: `${websiteBase}${docsSegment}/`,
  productName: appConfig.name,
  repositoryUrl: appConfig.repositoryUrl,
};
