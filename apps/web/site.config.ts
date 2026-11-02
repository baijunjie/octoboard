import appConfig from "../../config/app.json" with { type: "json" };

const origin = new URL(
  process.env.NUXT_PUBLIC_SITE_ORIGIN || appConfig.websiteUrl,
).origin;
const baseURL =
  `/${(process.env.NUXT_APP_BASE_URL || "/").replace(/^\/+|\/+$/g, "")}/`.replace(
    "//",
    "/",
  );

export default {
  origin,
  baseURL,
  url: `${origin}${baseURL}`,
};
