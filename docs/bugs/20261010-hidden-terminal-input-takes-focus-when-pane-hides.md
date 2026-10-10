> Severity: Moderate

## Symptom

With no session selected, keyboard focus lands on the hidden terminal input when the project pane is hidden.

## Reproduction steps

1. Build the packaged app (`pnpm build:app --bundles app` in `apps/desktop`) and launch it with no session selected.
2. Set the UI language to zh-Hans (简体中文) in Settings.
3. Open the project pane through the project row's menu ("项目 octoboard 的操作" / project actions) > "浏览文件"
   (Browse files).
4. Hide the project pane with its "隐藏项目面板" (Hide project pane) control.
5. Inspect where keyboard focus is (accessibility tree, for example Xcode's Accessibility Inspector).

## Expected vs. actual

- Expected: an input the user cannot see is not given focus when nothing is attached to it (WCAG 2.2 SC 2.4.3 Focus
  Order: focus moves in an order that preserves meaning and operability).
- Actual: after the project pane was hidden, keyboard focus landed on the hidden "Terminal input" field.

## Environment

- macOS, packaged app built from `main` at 487f99b plus the `fix/viewer-a11y-names` branch; zh-Hans UI.

## Scope of impact

Keyboard and VoiceOver users can land in a text field they cannot see after hiding the project pane. Workaround:
VoiceOver's own navigation keys.

## Leads

- Verified: hiding the project pane with no session selected put keyboard focus on the hidden "Terminal input"
  (accessibility tree), in a later run.
- Verified (seen once, not reproduced): in an earlier run, closing the sidebar's menu left focus on the hidden terminal
  field. In the later run, closing the sidebar's project row menu returned focus to the project row. Not an
  acceptance criterion.
- Inferred: the textarea exists and is focusable even with no session attached, which is how it takes focus when
  the element that had focus goes away. Not traced to the code that moves focus.

## Acceptance criteria

- [ ] With no session selected, hiding the project pane does not leave keyboard focus on the hidden terminal field.
