# Final Confirmation

> Goal: confirm that the completed translations work in the macOS application before shipping.
> Completion criteria: spot-check each newly translated language in the macOS app for clipped or overflowing text,
> and Arabic for correct right-to-left reading, including mixed-direction names and identifiers.

- [ ] Spot-check `ar`, `de`, `es`, `fr`, `hi`, `id`, `it`, `ja`, `ko`, `pt`, `ru`, `th`, `tr`, `vi` and
  `zh-Hant` in the macOS application, including Settings, dialogs, application menus, the menu bar icon's menu and
  notifications.
- [ ] Confirm that Arabic text reads correctly right to left in the macOS application, including Latin product
  names, paths and user-provided names.

## Verification still needed

The catalogs, generated native permission resources and browser gallery have been checked. No macOS application
bundle was built or launched for these checks, so browser results do not settle the native application checks above.

## Reference docs

- `docs/product/language.md`
- `docs/product/right-to-left-layout.md`
- `docs/product/macos-privacy-permissions.md`
