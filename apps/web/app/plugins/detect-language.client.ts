import { matchLanguage } from "../../../../packages/ui/src/i18n/matchLanguage";

const COOKIE_KEY = "octoboard_i18n_redirected";
const COOKIE_MAX_AGE = 60 * 60 * 24 * 365;

export default defineNuxtPlugin((nuxtApp) => {
  const i18n = nuxtApp.$i18n;
  const route = useRoute();
  const cookie = useCookie<string | null>(COOKIE_KEY, {
    path: "/",
    maxAge: COOKIE_MAX_AGE,
    sameSite: "lax",
  });

  nuxtApp.hook("app:mounted", async () => {
    if (route.meta.siteChrome === false) return;

    if (i18n.locale.value === i18n.defaultLocale) {
      const locales = i18n.locales.value;
      const remembered = locales.find(({ code }) => code === cookie.value);
      const language = matchLanguage(
        navigator.languages?.length ? navigator.languages : [navigator.language],
      );
      const preferred =
        remembered ?? locales.find(({ language: tag }) => tag === language);

      if (preferred && preferred.code !== i18n.locale.value) {
        await i18n.setLocale(preferred.code);
      }
    }

    watch(
      i18n.locale,
      (value) => {
        cookie.value = value;
      },
      { immediate: true },
    );
  });
});
