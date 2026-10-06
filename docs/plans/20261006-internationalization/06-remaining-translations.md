# Remaining Translations

> Goal: every catalog message — UI, daemon codes, native menu — is translated into the 15 languages not yet
> translated, before the application ships.
> Completion criteria: ar, de, es, fr, hi, id, it, ja, ko, pt-BR, ru, th, tr, vi and zh-Hant each have a complete
> catalog, with every CLDR plural category each requires, passing the catalog check; spot-checked in the macOS app in
> each language for clipped or overflowing text, and under ar for text that reads correctly right to left.

### Technical design

- [ ] Translations of the full English catalog into the 15 languages, each registered with the catalog check.

## Notes for the developer

- **Reusable capabilities**: the catalog, its completeness check and the zh-Hans translation as the worked example.
- **Development notes**: a translation is an internationalization resource and stays in its own language.
- **Reference docs**: `docs/product/language.md`, `packages/ui/README.md`.
