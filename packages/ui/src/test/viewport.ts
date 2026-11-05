/** Evaluates the width queries used by the app without treating every media query as a width query. */
export function matchesWidthQuery(query: string, width: number): boolean {
  const legacy = /^\((min|max)-width:\s*([\d.]+)px\)$/.exec(query);
  if (legacy) return legacy[1] === "min" ? width >= Number(legacy[2]) : width <= Number(legacy[2]);
  const range = /^\(width\s*(<=|>=|<|>)\s*([\d.]+)px\)$/.exec(query);
  if (!range) return false;
  const boundary = Number(range[2]);
  switch (range[1]) {
    case "<=": return width <= boundary;
    case ">=": return width >= boundary;
    case "<": return width < boundary;
    default: return width > boundary;
  }
}
