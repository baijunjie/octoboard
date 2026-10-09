import { FALLBACK_LANGUAGE, type Language, LANGUAGES } from "./languages";

/** Retired ISO 639 codes some systems still report, and the code that replaced each. */
const LEGACY_CODES: Record<string, string> = { in: "id", iw: "he", ji: "yi", jw: "jv", mo: "ro" };

/** The script a Chinese tag with a region but no script conventionally means; a region not listed
 * here implies no script, so the language alone decides. */
const CHINESE_REGION_SCRIPTS: Record<string, string> = { TW: "Hant", HK: "Hant", MO: "Hant", CN: "Hans", SG: "Hans" };

interface ParsedTag {
  language: string;
  script?: string;
}

/** Reads the language and script out of a BCP 47 tag, case-normalized; the region only matters
 * for Chinese, where it stands in for a missing script. Returns `undefined` for a malformed tag. */
function parseTag(tag: string): ParsedTag | undefined {
  const parts = tag.trim().replace(/_/g, "-").split("-");
  const first = parts[0]?.toLowerCase();
  if (!first || !/^[a-z]{2,3}$/.test(first)) return undefined;
  const language = LEGACY_CODES[first] ?? first;
  let script: string | undefined;
  let region: string | undefined;
  for (const part of parts.slice(1)) {
    if (!script && !region && /^[a-z]{4}$/i.test(part)) {
      script = part[0]!.toUpperCase() + part.slice(1).toLowerCase();
    } else if (!region && /^([a-z]{2}|\d{3})$/i.test(part)) {
      region = part.toUpperCase();
    } else {
      break;
    }
  }
  if (language === "zh" && !script && region) script = CHINESE_REGION_SCRIPTS[region];
  return { language, script };
}

const PARSED_LANGUAGES = LANGUAGES.map((language) => [language, parseTag(language)!] as const);

/** The first entry of the list one system language maps onto, or `undefined`. A different
 * language never matches, nor does a different script; when either side has no script, the
 * language alone decides. */
function matchOne(tag: string): Language | undefined {
  const wanted = parseTag(tag);
  if (!wanted) return undefined;
  for (const [language, offered] of PARSED_LANGUAGES) {
    if (offered.language !== wanted.language) continue;
    if (offered.script && wanted.script && offered.script !== wanted.script) continue;
    return language;
  }
  return undefined;
}

/** Maps the system's preferred languages, most preferred first, onto the list: the first one that
 * maps decides, and the fallback applies when none does. */
export function matchLanguage(preferred: readonly string[]): Language {
  for (const tag of preferred) {
    const match = matchOne(tag);
    if (match) return match;
  }
  return FALLBACK_LANGUAGE;
}
