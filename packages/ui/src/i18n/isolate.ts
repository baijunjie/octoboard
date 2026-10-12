/** Wraps `text` in a first-strong isolate, so a left-to-right value (a path, a branch name, a git
 * error) substituted into a sentence keeps its own order when the surrounding text runs right to
 * left. Without it the bidi algorithm reorders the value against the sentence: `“/Users/dev/app”`
 * reads as `”Users/dev/app/“`, and a trailing full stop or a closing parenthesis lands on the wrong
 * side. This is the plain-string twin of `<bdi dir="auto">`, for text that is not rendered as
 * React children: a tooltip, a `title`, a toast, an accessible name. */
export function isolate(text: string): string {
  return `\u2068${text}\u2069`;
}
