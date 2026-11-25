/** The characters Claude Code's prompt removes as invisible, as `apps/daemon/src/session/term/invisible.rs` lists them. */
const INVISIBLE: readonly (readonly [number, number])[] = [
  [0x00ad, 0x00ad],
  [0x034f, 0x034f],
  [0x061c, 0x061c],
  [0x115f, 0x1160],
  [0x17b4, 0x17b5],
  [0x180b, 0x180f],
  [0x200b, 0x200f],
  [0x2028, 0x202e],
  [0x2060, 0x206f],
  [0x3164, 0x3164],
  [0xfe00, 0xfe0f],
  [0xfeff, 0xfeff],
  [0xffa0, 0xffa0],
  [0xfff0, 0xfffb],
  [0x1107f, 0x1107f],
  [0x13430, 0x1343f],
  [0x16fe4, 0x16fe4],
  [0x1bca0, 0x1bca3],
  [0x1d173, 0x1d17a],
  [0xe0000, 0xe0fff],
];

/** Whether `character` (one code point) is one of the invisible ones. A list of ranges, not a character class: a class
 * holding joiners and selectors reads as combined characters. */
function isInvisible(character: string): boolean {
  const code = character.codePointAt(0) ?? 0;
  return INVISIBLE.some(([first, last]) => code >= first && code <= last);
}

/** The control characters the daemon strips: every one but a newline and a tab. */
const CONTROL = /[^\P{Cc}\n\t]/gu;

/**
 * Whether nothing but whitespace is left of `text` once its control and invisible characters are stripped: what the
 * daemon refuses as a missing prompt (`is_blank_once_stripped`), and what a launch would send as none. The emoji
 * exceptions the daemon's strip makes (a joiner or selector kept next to a pictograph) are not followed: a character
 * they keep always sits beside one that is not blank. `apps/daemon/testdata/blank_prompt_cases.json` pins this rule and
 * the daemon's together.
 */
export function isBlankPrompt(text: string): boolean {
  const visible = Array.from(text.replace(CONTROL, "")).filter((character) => !isInvisible(character));
  return visible.join("").trim() === "";
}
