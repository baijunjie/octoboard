# Website Development Plan

## Problem and approach

Octoboard needs a public project website. Its stack and hosting follow the reference project's website exactly: a Nuxt 4
site with the i18n module, generated to a static site and hosted on Cloudflare Pages, living in `apps/web`. This plan
brings the site up and deploys it; what the pages say is a separate matter.

## Key design decisions

- **Same stack and hosting as the reference website**: Nuxt 4, the i18n module, static generation, Cloudflare Pages, and
  the same build-time conventions (site configuration file, agent-readable Markdown versions of pages, robots and
  sitemap).
- **Static first**: dynamic parts (Pages Functions) are added only when something needs one.
- **All site prose is in English**; interface strings for other languages are internationalization resources and stay in
  their language.

## Milestone

> Goal: a deployed Octoboard website skeleton, built the way the reference website is.
> Completion criteria: the site type-checks and builds statically from `apps/web` with pnpm, is deployed on Cloudflare
> Pages on its own domain, and serves a home page.

### Technical design

- [ ] The Nuxt 4 site with i18n, a site configuration file, and the SEO and agent-readable outputs the reference site has.
- [ ] A static build and its Cloudflare Pages deployment settings.
- [ ] A home page with placeholder content.

### Implementation plan

- [ ] Bring the site up by following the reference website's structure, adopting only what applies to Octoboard.
- [ ] Set up the Pages project and domain.

### Notes for the developer

- **Development notes**: the Pages project and the domain need the owner's Cloudflare account; ask for them rather than
  guess. Part of the reference site (the Android wish vote, the join landing page, the app-association file) is specific
  to that product and does not carry over.
- **Reference docs**: the reference project's `apps/web` and its README.

## Open

- The domain.
- The site's content and which languages it ships.
- Whether the site will ever need a Pages Function.
