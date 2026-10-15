import { installPreviewStorage } from "./storage";
import { installPreviewKeyboardGuard } from "./keyboard";
import { installPreviewScrollBridge, withReportScrollBridge } from "./scrollBridge";
import { isLanguage, type Language } from "../i18n/languages";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { copyFor, createPreviewScenario, type PreviewScene } from "./scenarios";

// No application modules may evaluate before this: preferences must never touch the host origin.
installPreviewStorage();
const removeScrollBridge = installPreviewScrollBridge();
const removeKeyboardGuard = installPreviewKeyboardGuard();
window.addEventListener("pagehide", (event) => {
  if (!event.persisted) {
    removeScrollBridge();
    removeKeyboardGuard();
  }
});
const embedded = window.parent !== window;
const reportDocument = embedded ? withReportScrollBridge : (source: string) => source;
const params = new URLSearchParams(window.location.search);
const requestedLanguage = params.get("lang");
const language: Language = isLanguage(requestedLanguage)
  ? requestedLanguage
  : "en";
const requestedScene = params.get("scene");
const scene: PreviewScene =
  requestedScene === "report" || requestedScene === "changes"
    ? requestedScene
    : "overview";
const theme = params.get("theme") === "light" ? "light" : "dark";
const copy = copyFor(language);
const { scenario, output } = createPreviewScenario(scene, language);
localStorage.setItem(PREFERENCE_KEYS.language, language);
localStorage.setItem("heroui-theme", theme);
for (const [key, value] of Object.entries(scenario.preferences ?? {})) {
  localStorage.setItem(
    PREFERENCE_KEYS[key as keyof typeof scenario.preferences],
    String(value),
  );
}
document.documentElement.className = theme;
document.documentElement.dataset.theme = theme;

async function start(): Promise<void> {
  const { installFakeTerminal } = await import("../gallery/fakeTerminal");
  installFakeTerminal({ output, strict: true });
  const [
    { default: React },
    { createRoot },
    { App },
    { ErrorBoundary },
    { LanguageProvider },
    { PlatformProvider },
    { PageDocumentTransform },
    { DaemonProvider },
    { ThemeProvider },
    { createPreviewDaemon },
    { createUi },
  ] = await Promise.all([
    import("react"),
    import("react-dom/client"),
    import("../App"),
    import("../ErrorBoundary"),
    import("../i18n/react"),
    import("../platform/react"),
    import("../report/pageDocumentTransform"),
    import("../store"),
    import("../theme"),
    import("./daemon"),
    import("../gallery/interact"),
  ]);
  await import("../focusGuard");
  await import("../style.css");
  await import("./style.css");
  const daemon = createPreviewDaemon(scenario);
  createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <PlatformProvider value={{ kind: "browser" }}>
        <LanguageProvider>
          <ErrorBoundary>
            <ThemeProvider>
              {!embedded && (
                <div className="website-preview-label">{copy.demo}</div>
              )}
              <div className="website-preview-app">
                <PageDocumentTransform value={reportDocument}>
                  <DaemonProvider value={daemon}>
                    <App />
                  </DaemonProvider>
                </PageDocumentTransform>
              </div>
            </ThemeProvider>
          </ErrorBoundary>
        </LanguageProvider>
      </PlatformProvider>
    </React.StrictMode>,
  );
  const ui = createUi(document, language, daemon);
  for (const step of scenario.steps ?? []) await step(ui);
  await new Promise<void>((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
  );
  document.documentElement.dataset.previewReady = "true";
  window.parent.postMessage({ type: "octoboard-preview-ready" }, "*");
}

start().catch((error: unknown) => {
  console.error("Website preview failed", error);
  const root = document.getElementById("root")!;
  root.textContent = copy.unavailable;
  root.setAttribute("role", "alert");
  document.documentElement.dataset.previewError = "true";
  window.parent.postMessage({ type: "octoboard-preview-error" }, "*");
});
