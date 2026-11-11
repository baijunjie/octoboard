// The Web Worker that tests a page's `pattern` against a value (`patternCheck.ts` is its client). It answers each
// request with a verdict, and says it is ready once it has loaded, so that the client times a test from when the worker
// could start it rather than from when the worker itself started.
import { type PatternVerdict, testPattern } from "./patternTest";

export interface PatternRequest {
  id: number;
  pattern: string;
  text: string;
}

export type PatternReply = { ready: true } | { id: number; verdict: PatternVerdict };

self.addEventListener("message", (event: MessageEvent<PatternRequest>) => {
  const { id, pattern, text } = event.data;
  self.postMessage({ id, verdict: testPattern(pattern, text) } satisfies PatternReply);
});
self.postMessage({ ready: true } satisfies PatternReply);
