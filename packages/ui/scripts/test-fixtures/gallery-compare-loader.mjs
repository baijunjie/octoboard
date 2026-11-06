const fixture = new URL("./gallery-compare-resources.mjs", import.meta.url).href;
export async function resolve(specifier, context, nextResolve) {
  if (/\/node_modules\/playwright(?:-core)?\//.test(specifier)) return { url: fixture, shortCircuit: true };
  return nextResolve(specifier, context);
}
