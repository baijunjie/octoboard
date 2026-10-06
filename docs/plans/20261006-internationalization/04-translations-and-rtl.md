# Translations and Right-to-Left

> Goal: every catalog message — UI, daemon codes, native menu — is translated into all 16 non-English languages, and
> the UI is laid out correctly under ar.
> Completion criteria: every language's catalog has every message, with every CLDR plural category it requires; a
> check fails the build when a language is missing a message or a plural category; under ar the whole UI is mirrored,
> with directional icons and chevrons as the overview describes and the terminal unmirrored; spot-checked in the
> macOS app in each language for clipped or overflowing text.

### Technical design

- [ ] Translations of the full English catalog into ar, de, es, fr, hi, id, it, ja, ko, pt-BR, ru, th, tr, vi,
  zh-Hans and zh-Hant, including the daemon codes and the native menu.
- [ ] A completeness check over the catalogs (missing messages, missing plural categories, leftover placeholders),
  run as part of the UI's build.
- [ ] Layout under ar: the panes, the top bar, drawers and floating panes, the resize handle, toasts and dialogs follow
  the right-to-left direction; directional icons are mirrored; expand / collapse chevrons switch glyphs rather than
  rotate.
- [ ] The terminal keeps left-to-right rendering under ar.

## Notes for the developer

- **Reusable capabilities**: the catalog and fallback from milestone 1; the edge-fade component, whose start / end
  sides must follow the text direction.
- **Development notes**: layout code that names a physical side (left / right) for something that should follow the
  reading direction has to move to logical start / end; side-specific things that must not flip (the macOS traffic
  lights, the terminal) stay physical.
- **Reference docs**: `docs/product/window-layout.md`, `docs/memory/writing-ui-components.md`.
