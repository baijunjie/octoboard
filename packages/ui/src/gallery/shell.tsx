// The gallery's own page: the scenario list and the controls around the iframe that renders the
// chosen scenario (`frame.tsx`). A dev tool, so plain, English and outside the app's i18n and theme.
// What is chosen lives in the URL, so a link reproduces a view.
import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";

import { LANGUAGES, LANGUAGE_NAMES, isLanguage, type Language } from "../i18n/languages";
import { SCENARIOS } from "./fixtures";

const WIDTHS = [
  { label: "Wide", width: 1440 },
  { label: "Docked", width: 1148 },
  { label: "Narrow", width: 800 },
  { label: "Phone", width: 420 },
];

const MIN_WIDTH = 320;

interface View {
  scenario: string;
  theme: "light" | "dark";
  language: Language;
  /** Set by the user; otherwise the scenario's own width applies. */
  width?: number;
}

function readView(): View {
  const params = new URLSearchParams(window.location.search);
  const language = params.get("lang");
  const width = Number(params.get("width"));
  return {
    scenario: params.get("scenario") ?? SCENARIOS[0].id,
    theme: params.get("theme") === "dark" ? "dark" : "light",
    language: isLanguage(language) ? language : "en",
    width: Number.isFinite(width) && width > 0 ? width : undefined,
  };
}

function viewQuery(view: View): string {
  const params = new URLSearchParams({ scenario: view.scenario, theme: view.theme, lang: view.language });
  if (view.width) params.set("width", String(view.width));
  return `?${params}`;
}

/** The width in pixels as typed: applied on Enter or blur, so clearing the field to type another
 * number does not snap back, and a value under the smallest usable window is ignored. */
function WidthBox({ width, onCommit }: { width: number; onCommit: (width: number) => void }): React.ReactElement {
  const [text, setText] = useState(String(width));
  useEffect(() => setText(String(width)), [width]);
  const commit = () => {
    const typed = Number(text);
    if (typed >= MIN_WIDTH) onCommit(typed);
    else setText(String(width));
  };
  return (
    <input
      type="number"
      aria-label="Width in pixels"
      min={MIN_WIDTH}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}

function Gallery(): React.ReactElement {
  const [view, setView] = useState(readView);
  const [run, setRun] = useState(0);
  const scenario = SCENARIOS.find((s) => s.id === view.scenario) ?? SCENARIOS[0];
  const width = view.width ?? scenario.width ?? 1440;
  const update = (patch: Partial<View>) => setView((current) => ({ ...current, ...patch }));

  useEffect(() => {
    window.history.replaceState(null, "", viewQuery(view));
  }, [view]);

  const frameQuery = `?${new URLSearchParams({ scenario: scenario.id, theme: view.theme, lang: view.language })}`;
  const groups = Array.from(new Set(SCENARIOS.map((s) => s.group)));

  return (
    <>
      <nav aria-label="Scenarios">
        {groups.map((group) => (
          <React.Fragment key={group}>
            <h2>{group}</h2>
            {SCENARIOS.filter((s) => s.group === group).map((s) => (
              <a
                key={s.id}
                href={viewQuery({ ...view, scenario: s.id })}
                aria-current={s.id === scenario.id ? "page" : undefined}
                onClick={(e) => {
                  e.preventDefault();
                  update({ scenario: s.id });
                }}
              >
                {s.title}
              </a>
            ))}
          </React.Fragment>
        ))}
      </nav>
      <main>
        <header>
          <div className="group">
            {(["light", "dark"] as const).map((theme) => (
              <button key={theme} aria-pressed={view.theme === theme} onClick={() => update({ theme })}>
                {theme}
              </button>
            ))}
          </div>
          <label className="group">
            Language
            <select value={view.language} onChange={(e) => update({ language: e.target.value as Language })}>
              {LANGUAGES.map((language) => (
                <option key={language} value={language}>
                  {language} · {LANGUAGE_NAMES[language]}
                </option>
              ))}
            </select>
          </label>
          <div className="group">
            Width
            {WIDTHS.map(({ label, width: w }) => (
              <button key={label} aria-pressed={width === w} onClick={() => update({ width: w })}>
                {label} {w}
              </button>
            ))}
            <WidthBox width={width} onCommit={(w) => update({ width: w })} />
          </div>
          <a href={`gallery-frame.html${frameQuery}`} target="_blank" rel="noreferrer">
            Open alone
          </a>
          <button onClick={() => setRun((n) => n + 1)}>Reload</button>
          {scenario.description && <p className="description">{scenario.description}</p>}
        </header>
        <div className="stage">
          <iframe
            key={`${run}${frameQuery}`}
            title={scenario.title}
            src={`gallery-frame.html${frameQuery}`}
            style={{ width }}
          />
        </div>
      </main>
    </>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<Gallery />);
