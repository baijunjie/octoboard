const origin = new URL(
  process.env.NUXT_PUBLIC_SITE_ORIGIN || "https://baijunjie.github.io",
).origin;
const baseURL =
  `/${(process.env.NUXT_APP_BASE_URL || "/octoboard/").replace(/^\/+|\/+$/g, "")}/`.replace(
    "//",
    "/",
  );

export default {
  origin,
  baseURL,
  url: `${origin}${baseURL}`,
};
