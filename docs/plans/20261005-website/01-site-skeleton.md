# Site Skeleton

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
