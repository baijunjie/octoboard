const origin = new URL(
  process.env.NUXT_PUBLIC_SITE_ORIGIN || "https://octoboard.dev",
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
