// The visitor's answer to the analytics prompt. Google Analytics is configured to load only when
// requested, and this is what requests it: on a page load once the answer is "granted", or when the
// visitor allows it. The answer is kept for a year in a same-site cookie; with none, the prompt is
// shown.
type Consent = "granted" | "denied";

const COOKIE_KEY = "octoboard_analytics_consent";
const COOKIE_MAX_AGE = 60 * 60 * 24 * 365;
// The cookies Google Analytics sets: `_ga`, and `_ga_<container>` per measurement ID.
const ANALYTICS_COOKIE = /^_ga(_|$)/;

export function useAnalyticsConsent() {
  const nuxtApp = useNuxtApp();
  const cookie = useCookie<Consent | null>(COOKIE_KEY, {
    path: "/",
    maxAge: COOKIE_MAX_AGE,
    sameSite: "lax",
  });
  const { initialize, enableAnalytics, disableAnalytics } = useGtag();
  // Shown only on the client: the static pages are the same for every visitor, so the prerendered
  // HTML never carries the prompt, and a visitor who has answered never sees it flash.
  const open = useState("analyticsConsentOpen", () => false);
  // The control that reopened the prompt, which gets focus back once it is answered.
  const opener = useState<HTMLElement | null>("analyticsConsentOpener", () => null);
  // Counts the reopenings, each of which moves focus to the prompt, open already or not.
  const focusRequests = useState("analyticsConsentFocusRequests", () => 0);

  function load() {
    enableAnalytics();
    // `initialize` adds the script through `useHead`, which needs the Nuxt context a click handler
    // does not run in. Outside a component's scope, as here, the script belongs to the app.
    nuxtApp.runWithContext(() => initialize());
  }

  // Run once the app is mounted: loads Google Analytics after an earlier "granted", or, where the
  // page may ask, asks.
  function restore(mayAsk: boolean) {
    if (cookie.value === "granted") load();
    else if (cookie.value !== "denied" && mayAsk) open.value = true;
  }

  // `returnTo` is where focus goes when the prompt was not reopened, since the button pressed goes
  // away with the prompt; null leaves it.
  function choose(consent: Consent, returnTo: HTMLElement | null = null) {
    cookie.value = consent;
    open.value = false;
    if (consent === "granted") {
      load();
    } else {
      // An earlier "granted" may have loaded it on this page: stop it sending anything more and
      // remove what it stored.
      disableAnalytics();
      removeAnalyticsCookies();
    }
    (opener.value ?? returnTo)?.focus();
    opener.value = null;
  }

  function reopen(event: Event) {
    opener.value = event.currentTarget as HTMLElement;
    open.value = true;
    focusRequests.value++;
  }

  return { open, focusRequests, restore, choose, reopen };
}

// Google Analytics sets its cookies on the widest domain it can (`.octoboard.dev` for the site), so
// each is expired on the page's host and on every parent domain of it.
function removeAnalyticsCookies() {
  const names = document.cookie
    .split(";")
    .map((entry) => entry.split("=")[0]!.trim())
    .filter((name) => ANALYTICS_COOKIE.test(name));
  const labels = location.hostname.split(".");
  const domains = labels.map((_, index) => labels.slice(index).join("."));
  for (const name of names) {
    document.cookie = `${name}=; Max-Age=0; path=/`;
    for (const domain of domains) {
      document.cookie = `${name}=; Max-Age=0; path=/; domain=${domain}`;
    }
  }
}
