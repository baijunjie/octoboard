# Website Languages

> Goal: the website ships the same 17 languages and sends a browser to its language by the same mapping rules.
> Completion criteria: the site builds statically with all 17 languages, its language menu lists them in tag order,
> a visitor's browser languages map to one of them by the overview's rules (legacy-code normalization aside), falling
> back to English; ar pages are mirrored.

### Technical design

- [ ] The website's language list is the overview's 17 languages, in tag order, with English as the fallback.
- [ ] Browser-language detection maps onto the list with the overview's rules, without the legacy-code normalization.
- [ ] The interface strings of the site translated into every language, and ar pages laid out right to left.

## Notes for the developer

- **Development notes**: this milestone needs the website itself to exist first (its own development plan builds the
  site skeleton with its i18n module).
- **Reference docs**: `docs/plans/20261005-website/README.md`, `apps/web/README.md`.
