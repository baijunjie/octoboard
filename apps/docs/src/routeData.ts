// Starlight route middleware (`routeMiddleware` in `astro.config.ts`).
import { defineRouteMiddleware } from "@astrojs/starlight/route-data";

export const onRequest = defineRouteMiddleware(({ locals }) => {
  const route = locals.starlightRoute;
  // The 404 page is built once, at the docs' root, but Starlight gives it a language alternate for every locale, at
  // `<locale>/404/` pages that do not exist.
  if (route.id === "404") {
    route.head = route.head.filter((tag) => !(tag.tag === "link" && tag.attrs?.rel === "alternate"));
  }
});
