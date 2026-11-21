# Focus and key handling in the UI

The rules are in `docs/conventions/ui-accessibility.md`, "Focus" and "Key handling"; the test helpers
they need (`packages/ui/src/components/dialog/dialogTestSupport.ts`, `packages/ui/src/test/`) say on their doc comments
when to use them. jsdom and the gallery miss most of these defects in their own ways; what follows is where each one
shows and how a test reaches it.

## jsdom moves focus differently from WebKit: make the test do what the browser does

- jsdom does not drop focus when an element or an ancestor is given `hidden` (only an element hidden already refuses
  `focus()`), so a test of a hide handoff asserts the exact destination; one asserting only that focus is not on
  `<body>` passes with no handoff at all.
- jsdom never blurs a disabled element, so a test of `useFocusLeavingWhenDisabled` reaches the WebKit case only by
  dispatching the `focusout` and reporting `<body>` as the active element by hand; a test of a field `useFocusHandoff`
  covers blurs the control itself as it disables it.
- `packages/ui/src/App.focusHandoff.test.tsx` drives removals only for the terminal pane (its history's owner line
  included), the update notice and the connection banner. A change that makes a component render less can stop its
  `useFocusHandoff` with nothing failing, so a new handoff gets a test of its own.
- jsdom runs no exit animation, so a component test passes against a row menu's focus return timed from the close;
  settle such a return in a browser on the gallery page, driven from the keyboard.
- jsdom has no `inert`, so a press sent to a control behind an open React Aria modal runs handlers a browser never
  reaches: send the press to `document.body`, blur what had focus behind the overlay, and send the target only the
  events that follow the close.
- A jsdom test does not assert where focus lands after a session is selected: the terminal attach moves it. Assert focus
  only for moves that select no session.
- A test of focus after a handoff that React commits asserts `data-focus-visible` on the element React Aria marks, after
  `await act(async () => {})`, and sends the change as a new array rather than an element replaced inside the fixture's
  array.
- The file viewer's renderer has its shadow root mocked away in jsdom, so a viewer test asserts only what a copy or
  select-all handler does to the DOM; what a selection over code copies is settled by copying in a real browser.
- A test of a focus move that a mouse press sets off replays the press's `mousedown` default with the page's own
  `HTMLElement.prototype.focus`, saved in `vi.hoisted` before react-aria patches it, called on the pressed element right
  after `pointerdown`; and it reads `getInteractionModality()` before sending `pointerup`, since a real button is still
  held when the next frame runs. A stand-in calling `element.focus()` goes through react-aria's patch, which counts as
  an input event, and `pointerup` sets the modality back to `pointer`, so either one hides a scripted focus inside
  `pointerdown` leaving the modality `virtual` (the next focus draws a keyboard ring); a test asserting only
  `document.activeElement` passes against it too. Copy `mouseDown` in
  `packages/ui/src/terminal/shell/ShellTerminalPanel.test.tsx`.

## Test a dialog's focus under each press that breaks it

- A dialog's opening focus: the mount refocus and replaced content fail only after a press without a pointer
  (`framesAsTimers()`, under `StrictMode` as well); the DOM element's `autoFocus` fails only after a keyboard or mouse
  press, since under a press without a pointer the outer dialog's pull-back is put off until it is inert (the "opens the
  directory browser on its path field" cases in `packages/ui/src/dialogs/ProjectDialog.test.tsx`).
- A dialog another dialog opens as it mounts: test under all three modalities (`framesAsTimers(modality)`), since each
  leaves React Aria's restore somewhere else, and the effect-driven close with the store notified outside `act`
  (`notifyLater` in `packages/ui/src/settings/SettingsDialog.dialogsInFront.test.tsx`).
- A modal raised over a dialog: a test without `framesAsTimers()` passes against the timer version. Use an opener
  outside focus, mount the `openable` first, `settleFrames()` and `StrictMode`, assert the modal is not inside
  `[aria-hidden], [inert]`, and cover the dialog covered by a later one. Also raise the modal in the same `act` as
  opening the dialog underneath (`openable(dialog, true)`). React Aria's half-second take-back is
  reached only with a `transitionrun` that never ends, dispatched on an element of the dialog underneath, and awaiting
  `trackMountRefocus()` armed once the modal is raised. Copy the "over a dialog opened without a pointer" cases in
  `packages/ui/src/shell/ErrorAlerts.test.tsx`.
- A rule gated on window focus: only a test of the window-left case catches the ungated rule; no gallery scenario leaves
  the window for another application.

## Reading an Escape or press defect in the running page

- When Escape does nothing somewhere, what predicts it is "a tooltip with a trigger is open", not which control holds
  focus: every icon-only control's tooltip opens on keyboard focus, so one Tab inside any dialog or view reaches that
  state.
- The pane's Escape listener (`layout/usePaneToggles.ts`) is installed only while an overlay is up, so a defect in it
  shows only with the sidebar's drawer open below the layout breakpoint, or a pane floated in on hover. A gallery
  scenario at a normal width, with the sidebar docked and nothing floating, never reaches it.
- A surface that stays undimmed while a dialog is open yet takes no pointer event has its top-layer marking but sits
  under the modal's backdrop, which takes the presses: lower the backdrop. It is not the marking failing.
