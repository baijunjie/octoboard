import { installPreviewStorage } from "../storage";

// Preferences are initialized by imports, before any component mounts.
installPreviewStorage();
const params = new URLSearchParams(location.search);
const feature = params.get("feature") ?? "managedAccounts";

async function start(): Promise<void> {
  const { isLanguage } = await import("../../i18n/languages");
  const { PREFERENCE_KEYS } = await import("../../preferenceKeys");
  const requestedLanguage = params.get("lang");
  const language = isLanguage(requestedLanguage) ? requestedLanguage : "en";
  localStorage.setItem(PREFERENCE_KEYS.language, language);
  localStorage.setItem("heroui-theme", "dark");
  const [{ default: React }, { createRoot }, { FeatureView }, { LanguageProvider }, { PlatformProvider }, { ThemeProvider }] = await Promise.all([
    import("react"),
    import("react-dom/client"),
    import("./views"),
    import("../../i18n/react"),
    import("../../platform/react"),
    import("../../theme"),
  ]);
  await import("../../style.css");
  await import("./style.css");
  createRoot(document.getElementById("root")!).render(
    <PlatformProvider value={{ kind: "browser" }}>
      <LanguageProvider><ThemeProvider><FeatureView feature={feature} /></ThemeProvider></LanguageProvider>
    </PlatformProvider>,
  );
}

export function ready(): void {
  if (document.documentElement.dataset.featureReady) return;
  document.documentElement.dataset.featureReady = "true";
  window.parent.postMessage({ type: "octoboard-feature-ready", feature }, "*");
}

function failed(error: unknown): void {
  console.error("Feature view failed", error);
  document.documentElement.dataset.featureError = "true";
  window.parent.postMessage({ type: "octoboard-feature-error", feature }, "*");
}
window.addEventListener("error", (event) => failed(event.error));
window.addEventListener("unhandledrejection", (event) => failed(event.reason));
start().catch(failed);
