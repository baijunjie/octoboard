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
- **The site ships the desktop application's 17 languages** (ar, de, en, es, fr, hi, id, it, ja, ko, pt-BR, ru, th, tr,
  vi, zh-Hans, zh-Hant), in tag order, with English as the fallback, and maps a browser's languages onto them by the
  same rules the application uses for the system language.

## Milestones

1. [Site skeleton](01-site-skeleton.md)
2. [Languages](02-languages.md)

## Open

- The domain.
- The site's content.
- The site's URL scheme per language and its default language.
- Whether the site will ever need a Pages Function.
