> Severity: Minor

## Symptom

With a zh-Hans UI, VoiceOver speaks the roles of the app's controls in English ("tab", "button", "region", "web
dialog", "splitter", "menu item", "text entry area"), while Safari on the same Mac speaks them in Chinese.

## Reproduction steps

1. Build the packaged app (`pnpm build:app --bundles app` in `apps/desktop`) and launch it; set the language to
   简体中文 in Settings.
2. Turn VoiceOver on with Cmd+F5 (system language and VoiceOver voice Chinese).
3. With the VO cursor, move over a tab, a button, the file viewer's code region, a dialog, a pane resize handle, a
   menu item and the terminal's input.
4. Listen to the role words. Open the same kinds of controls in Safari and compare.

## Expected vs. actual

- Expected: role words are spoken in the UI language, as VoiceOver does for web content in Safari (the UI language
  is the user's choice: `docs/product/language.md`, "Choosing the language"; the UI should not be spoken half in
  another language).
- Actual: the app's roles are English; Safari on the same Mac says "按钮", "主要".

## Environment

- macOS, packaged app built from `main` at 487f99b plus the `fix/viewer-a11y-names` branch; zh-Hans UI.
- Control: a plain WKWebView test window with no Chinese localization is also English.

## Scope of impact

Chinese-speaking VoiceOver users hear English role words. No workaround.

## Leads

- Verified: Safari on the same Mac speaks the roles in Chinese; a plain WKWebView window without localization speaks
  them in English.
- Verified: the repository declares no bundle localization: `apps/desktop/src-tauri/tauri.conf.json` has no
  `CFBundleLocalizations` or localization setting under `bundle` or `bundle.macOS`, and the repository holds no
  `Info.plist` and no `.lproj` directory under `apps/desktop`.
- Inferred: VoiceOver takes the role words' language from the app bundle's declared localizations, so an app that
  declares only English is spoken in English. Not tested against a built bundle's generated `Info.plist`.

## Acceptance criteria

- [ ] In the packaged app with a zh-Hans UI and a Chinese VoiceOver voice, the roles of a tab, a button, a region, a
  dialog, a splitter, a menu item and a text entry area are spoken in Chinese.
