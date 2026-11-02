import { installPreviewStorage } from "./storage";
import { installPreviewKeyboardGuard } from "./keyboard";
import { installPreviewFocusGuard } from "./focus";
import { installPreviewInteractionBridge, withReportInteractionBridge } from "./interactionBridge";
import { installPreviewScrollBridge, withReportScrollBridge } from "./scrollBridge";
import { PREVIEW_MESSAGE, postPreviewMessage } from "./messages";
import { isLanguage, type Language } from "../i18n/languages";
import { writeStartingPreferences } from "../fixtures/preferences";
import { afterFrames, applyDocumentTheme } from "../fixtures/pageFrame";
import { copyFor, createPreviewScenario, type PreviewScene } from "./scenarios";

// The static imports above evaluate before this, so none of them may read or keep a preference as
// it is evaluated (the language among them): it would be settled from, and kept in, the host
// origin's storage. Anything that does is imported in `start()`.
installPreviewStorage();
const removeScrollBridge = installPreviewScrollBridge();
const removeKeyboardGuard = installPreviewKeyboardGuard();
// Ahead of the focus guard, so the homepage still hears of a Tab the guard keeps from the application.
const removeInteractionBridge = installPreviewInteractionBridge();
const removeFocusGuard = installPreviewFocusGuard();
window.addEventListener("pagehide", (event) => {
  if (!event.persisted) {
    removeScrollBridge();
    removeKeyboardGuard();
    removeFocusGuard();
    removeInteractionBridge();
  }
});
const embedded = window.parent !== window;
const reportDocument = embedded
  ? (source: string) => withReportInteractionBridge(withReportScrollBridge(source))
  : (source: string) => source;
const params = new URLSearchParams(window.location.search);
const requestedLanguage = params.get("lang");
const language: Language = isLanguage(requestedLanguage)
  ? requestedLanguage
  : "en";
const requestedScene = params.get("scene");
const scene: PreviewScene =
  requestedScene === "focus" || requestedScene === "changes"
    ? requestedScene
    : "overview";
const theme = params.get("theme") === "light" ? "light" : "dark";
const copy = copyFor(language);
const { scenario, output } = createPreviewScenario(
  scene,
  language,
  !window.matchMedia("(prefers-reduced-motion: reduce)").matches,
);
writeStartingPreferences(localStorage, { language, theme, preferences: scenario.preferences });
applyDocumentTheme(document, theme);

async function start(): Promise<void> {
  const { installFakeTerminal } = await import("../fixtures/fakeTerminal");
  // A terminal opens only once the app runs, by which time the daemon below exists.
  installFakeTerminal({
    output: (session) => output(session, daemon.store.getState()),
    strict: true,
  });
  const [
    { default: React },
    { createRoot },
    { App },
    { AppRoot },
    { PageDocumentTransform },
    { createPreviewDaemon },
    { createUi },
    { setInteractionModality },
  ] = await Promise.all([
    import("react"),
    import("react-dom/client"),
    import("../App"),
    import("../AppRoot"),
    import("../report/pageDocumentTransform"),
    import("./daemon"),
    import("../fixtures/interact"),
    import("react-aria"),
  ]);
  await import("../input/focusGuard");
  await import("../style.css");
  await import("./style.css");
  const daemon = createPreviewDaemon(scenario);
  createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <AppRoot platform={{ kind: "browser" }} daemon={daemon} nativeSync={false}>
        {!embedded && <div className="website-preview-label groove-b">{copy.demo}</div>}
        <div className="website-preview-app">
          <PageDocumentTransform value={reportDocument}>
            <App />
          </PageDocumentTransform>
        </div>
      </AppRoot>
    </React.StrictMode>,
  );
  const ui = createUi(document, language, daemon);
  for (const step of scenario.steps) await step(ui);
  // The steps' scripted clicks read as an assistive technology's, which shows the focused control's
  // ring and tooltip. They stand in for a visitor's presses, so focus where they left it is put
  // there again as a pointer's: a phone's visitor would otherwise find a tooltip up on load.
  const held = document.activeElement;
  setInteractionModality("pointer");
  if (held instanceof HTMLElement && held !== document.body) {
    held.blur();
    held.focus({ preventScroll: true });
  }
  await afterFrames(200);
  document.documentElement.dataset.previewReady = "true";
  postPreviewMessage(window.parent, { type: PREVIEW_MESSAGE.ready });
}

start().catch((error: unknown) => {
  console.error("Website preview failed", error);
  const root = document.getElementById("root")!;
  root.textContent = copy.unavailable;
  root.setAttribute("role", "alert");
  document.documentElement.dataset.previewError = "true";
  postPreviewMessage(window.parent, { type: PREVIEW_MESSAGE.error });
});
