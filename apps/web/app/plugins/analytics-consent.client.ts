// Settles analytics once the app is mounted: loads it for a visitor who allowed it, or asks one who
// has not answered. Running here rather than in the prompt's component keeps the loaded script
// owned by the app, so it outlives the prompt and is never added twice.
export default defineNuxtPlugin((nuxtApp) => {
  const route = useRoute();
  nuxtApp.hook("app:mounted", () => {
    nuxtApp.runWithContext(() =>
      useAnalyticsConsent().restore(route.meta.siteChrome !== false),
    );
  });
});
