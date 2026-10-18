import { Globe } from "lucide-react";

import type { RepositoryBrand } from "../repositoryLink";

const MARKS: Record<RepositoryBrand, string[]> = {
  github: ["M12 .3a12 12 0 0 0-3.8 23.4c.6.1.8-.3.8-.6v-2.3c-3.3.7-4-1.4-4-1.4-.5-1.4-1.3-1.7-1.3-1.7-1.1-.8.1-.8.1-.8 1.2.1 1.8 1.3 1.8 1.3 1.1 1.8 2.8 1.3 3.5 1 .1-.8.4-1.3.8-1.6-2.7-.3-5.5-1.3-5.5-5.9 0-1.3.5-2.4 1.2-3.2-.1-.3-.5-1.5.1-3.2 0 0 1-.3 3.3 1.2a11.4 11.4 0 0 1 6 0C17.3 5 18.3 5.3 18.3 5.3c.6 1.7.2 2.9.1 3.2.8.8 1.2 1.9 1.2 3.2 0 4.6-2.8 5.6-5.5 5.9.4.4.8 1.1.8 2.2v3.3c0 .3.2.7.8.6A12 12 0 0 0 12 .3Z"],
  gitlab: ["m12 22 4.4-13H7.6L12 22Z", "M12 22 .7 13.8 3.8 4l3.8 5L12 22Z", "M12 22 23.3 13.8 20.2 4l-3.8 5L12 22Z", "M3.8 4 7.6 9H1.9L3.8 4Z", "M20.2 4l1.9 5h-5.7l3.8-5Z"],
  bitbucket: ["M2 2a1 1 0 0 0-1 1.2l3 18a1 1 0 0 0 1 .8h14a1 1 0 0 0 1-.8l3-18A1 1 0 0 0 22 2H2Zm7 13L8 9h8l-1 6H9Z"],
};

/** Hosting marks are decorative: the adjacent menu label names the host. */
export function RepositoryIcon({ brand }: { brand?: RepositoryBrand }) {
  if (!brand) return <Globe className="size-4 shrink-0" aria-hidden="true" />;
  return <svg viewBox="0 0 24 24" className="size-4 shrink-0" fill="currentColor" fillRule="evenodd" aria-hidden="true">{MARKS[brand].map((d, index) => <path key={index} d={d} />)}</svg>;
}
