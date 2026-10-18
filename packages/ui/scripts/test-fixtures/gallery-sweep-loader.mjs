// Keep lifecycle tests independent of a downloaded browser and the checkout's dependency optimizer.
const fixture = new URL("./gallery-sweep-resources.mjs", import.meta.url).href;

export async function resolve(specifier, context, nextResolve) {
  if (specifier === "vite" || /\/node_modules\/playwright(?:-core)?\//.test(specifier)) {
    return { url: fixture, shortCircuit: true };
  }
  return nextResolve(specifier, context);
}
