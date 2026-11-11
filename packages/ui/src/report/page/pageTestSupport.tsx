// Shared by the tests of the page's components: a page rendered the way the panel renders it, and the queries over it.
import { act, useMemo, useState } from "react";
import { onTestFinished, vi } from "vitest";

import type { PlatformAdapter } from "../../platform";
import type { PageContent, SubmittedValue } from "../../protocol";
import { BROWSER_HOST } from "../../test/hosts";
import { mountHost } from "../../test/mountHost";
import { defaultValues, type FormValues } from "./formModel";
import { testPattern } from "./patternTest";
import type { PatternReply, PatternRequest } from "./patternWorker";

import { PageView } from "./PageView";

export interface PageOptions {
  submission?: Record<string, SubmittedValue> | null;
  history?: boolean;
  pending?: boolean;
  platform?: PlatformAdapter;
}

/** Stands in for the pattern worker, which jsdom has no Workers to run: it answers from the same test (`testPattern`), a
 * microtask later, and says it is ready as a real one does. */
export function installPatternWorker(): void {
  class FakeWorker {
    onmessage: ((event: MessageEvent<PatternReply>) => void) | null = null;
    constructor() {
      queueMicrotask(() => this.onmessage?.({ data: { ready: true } } as MessageEvent<PatternReply>));
    }
    postMessage({ id, pattern, text }: PatternRequest) {
      queueMicrotask(() => this.onmessage?.({ data: { id, verdict: testPattern(pattern, text) } } as MessageEvent<PatternReply>));
    }
    terminate() {}
  }
  const previous = Object.getOwnPropertyDescriptor(globalThis, "Worker");
  vi.stubGlobal("Worker", FakeWorker);
  onTestFinished(() => {
    if (previous) Object.defineProperty(globalThis, "Worker", previous);
    else Reflect.deleteProperty(globalThis, "Worker");
  });
}

/** What owns a page in the panel, for a page shown on its own: it holds the values the user enters, and the submitter
 * that was pressed. */
function PageOwner({ content, onSubmit, ...state }: {
  content: PageContent;
  submission: Record<string, SubmittedValue> | null;
  history: boolean;
  pending: boolean;
  onSubmit: (data: Record<string, SubmittedValue>) => void;
}) {
  const [draft, setDraft] = useState<FormValues>();
  const [pressed, setPressed] = useState<string>();
  const values = useMemo(() => draft ?? defaultValues(content), [draft, content]);
  return (
    <PageView
      content={content}
      {...state}
      pressed={pressed}
      draft={values}
      onDraft={(update) => setDraft((held) => update(held ?? defaultValues(content)))}
      onSubmit={(data, press) => {
        setPressed(press.key);
        onSubmit(data);
      }}
    />
  );
}

/** Renders `content` as a page, with the settled first render done; `rerender` changes the page's state in place. */
export async function renderPage(content: PageContent, { submission = null, history = false, pending = false, platform }: PageOptions = {}) {
  installPatternWorker();
  const onSubmit = vi.fn<(data: Record<string, SubmittedValue>) => void>();
  const view = (next: Partial<PageOptions> = {}) => (
    <PageOwner content={content} submission={next.submission ?? submission} history={next.history ?? history} pending={next.pending ?? pending} onSubmit={onSubmit} />
  );
  const host = mountHost(view(), { platform });
  await act(async () => {});
  return { host, onSubmit, rerender: (next: Partial<PageOptions>) => act(() => host.render(view(next))) };
}

/** The platform of a browser whose clipboard is `clipboard`. */
export function browserWith(clipboard: PlatformAdapter["clipboard"]): PlatformAdapter {
  return { kind: "browser", host: BROWSER_HOST, clipboard };
}

export const buttonNamed = (scope: ParentNode, name: string) => [...scope.querySelectorAll("button")].find((candidate) => candidate.textContent === name)!;
