/**
 * The key the renderer's worker pool files a highlighted result under, so code shown again — the
 * same file reopened, a fence redrawn, a diff viewed again — reuses the result instead of being
 * tokenized anew. The library keys only by what it is handed and compares no contents, so the key
 * carries a hash of the text: a text that changes gets a key of its own and is never shown with an
 * earlier text's highlighting.
 */
export function highlightCacheKey(kind: string, name: string, language: string | undefined, text: string): string {
  return `${kind}:${language ?? ""}:${text.length}:${hashText(text)}:${name}`;
}

/** cyrb53: a fast, well-distributed 53-bit string hash. Not cryptographic; with the length and the
 * name beside it in the key, a collision between two texts shown in the viewer is not a concern. */
function hashText(text: string): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let index = 0; index < text.length; index++) {
    const code = text.charCodeAt(index);
    h1 = Math.imul(h1 ^ code, 2654435761);
    h2 = Math.imul(h2 ^ code, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return (4294967296 * (2097151 & h2) + (h1 >>> 0)).toString(36);
}
