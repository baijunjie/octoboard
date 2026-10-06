# Website Languages

> Goal: the website ships the desktop application's 17 languages and sends a browser to its language by the same
> mapping rules.
> Completion criteria: the site builds statically with all 17 languages, its language menu lists them in tag order,
> a visitor's browser languages map to one of them by the rules below, falling back to English; ar pages are mirrored.

### Technical design

- [ ] The website's language list is the 17 languages, in tag order, with English as the fallback (named on its own,
  never taken as the list's first entry).
- [ ] Browser-language detection maps onto the list with the application's rules, except legacy-code normalization
  (browsers already send current codes): a different language never matches; Chinese with only a region gets the
  script that region conventionally uses (TW / HK / MO → Hant, CN / SG → Hans, other regions none); a script conflict
  never matches, and when either side has no script the language alone decides (pt-PT and a bare `pt` map to pt-BR);
  among several matches the earlier one in tag order wins; a language that maps to nothing is dropped.
- [ ] The interface strings of the site translated into every language, and ar pages laid out right to left.

## Notes for the developer

- **Development notes**: needs the site skeleton from milestone 1. The application's own implementation of the mapping
  is the reference to keep the two in step with.
- **Reference docs**: `docs/product/language.md`, `apps/web/README.md`.
