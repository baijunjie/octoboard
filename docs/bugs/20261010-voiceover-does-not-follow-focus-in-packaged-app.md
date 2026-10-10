> Severity: Major

## Symptom

In the packaged app, VoiceOver sometimes does not follow keyboard focus: moving focus produces no speech and the
VoiceOver cursor stays where it was. It happened in one run and not in a later one.

## Reproduction steps

1. Build the packaged app (`pnpm build:app --bundles app` in `apps/desktop`) and launch it with the UI language
   zh-Hans.
2. Turn VoiceOver on with Cmd+F5, then bring the Octoboard window forward. This is the order of the run that
   reproduced it; it reproduced in 1 of 2 runs, and in the other run the app had been brought forward before these
   steps and VoiceOver followed focus.
3. Open the project pane through the project's menu ("浏览文件" / Browse files) and switch to Git mode.
4. Click the "未提交" / "比较" (Uncommitted / Compare) tabs, then switch between them with the Left / Right arrow keys.
5. Open the worktree selector popover, then the branch selector popover (focus goes to the first option).
6. Open a change in the file viewer (focus goes to the dialog).
7. After each step, listen for VoiceOver speech and watch where the VoiceOver cursor is (VoiceOver Utility's caption
   panel, or `content of last phrase` through VoiceOver's AppleScript interface).

## Expected vs. actual

- Expected: VoiceOver announces the element that takes keyboard focus and moves its cursor to it
  (`docs/memory/writing-ui-components.md`, "The UI meets WCAG 2.2 AA", "Name" and "Roles and states": a control's
  name and state are only useful if assistive technology reports them when focus lands on it).
- Actual: no speech and no cursor movement for steps 4 to 6, although the accessibility tree confirms focus moved each
  time; bringing the window forward puts VoiceOver on the window's "最小化 按钮" (minimize button), not on the focused
  element.

## Environment

- macOS, packaged app built from `main` at 487f99b plus the `fix/viewer-a11y-names` branch; zh-Hans UI; VoiceOver
  speech read through VoiceOver's AppleScript `content of last phrase`.
- Frequency: 1 of 2 runs. In the later run (new build, pid 29303, same Mac, after the app had been brought forward)
  VoiceOver followed focus: bringing the window forward said "Octoboard 网页内容 有键盘焦点", and the tab clicks, the
  branch popover, and the viewer opening and closing were all announced.
- Controls, all with the same Mac and VoiceOver settings:
  - A plain WKWebView test window running the same UI code (the UI gallery build) announces the popover opening
    ("从 web dialog 包含3个项目 feature/ranking-experiments 9e8d7c6 （1/4）") and arrowing through the change list.
  - A plain WKWebView window configured like Octoboard's (transparent, vibrancy, full-size content view) also
    announces focus.

## Scope of impact

VoiceOver users who move with Tab and the arrow keys hear nothing and cannot tell where focus is, in the runs where it
happens. Workaround: VoiceOver's own navigation keys (VO cursor), which read the content correctly.

## Leads

- Verified: the plain WKWebView window with the same UI code announces focus.
- Inferred: the UI code is not the cause.
- Verified: the plain WKWebView window styled like Octoboard's (transparent, vibrancy, full-size content view)
  announces focus.
- Inferred: the window style is not the cause.
- Verified: reading with the VO cursor works in the packaged app.
- Unknown: the condition under which VoiceOver does not follow focus; the only recorded difference between the two
  runs is whether the app had been brought forward first.
- Verified (earlier observation): the packaged app's window exposed no web content to the accessibility API until the
  app had been activated once.
- Inferred (lead for the trigger): it may depend on whether the app had been activated before VoiceOver started,
  matching the earlier observation that web content was not exposed until the app was activated once.
- Inferred: something in the Tauri / tao / wry shell or the app's native setup differs from a plain WKWebView window
  and keeps VoiceOver from tracking the web view's focus. Root cause not established.

## Acceptance criteria

- [ ] In the packaged app, clicking a tab or moving between tabs with the arrow keys makes VoiceOver announce the
  tab and follow focus.
- [ ] In the packaged app, opening the worktree or branch selector popover makes VoiceOver announce the popover and
  its first option.
- [ ] In the packaged app, opening a change in the file viewer makes VoiceOver announce the dialog.
- [ ] Bringing the window forward puts the VoiceOver cursor on the focused element, not on the minimize button.
