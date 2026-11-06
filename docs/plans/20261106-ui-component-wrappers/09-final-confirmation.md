# 09 Final confirmation

> Goal: confirm what could not be verified while the milestones were built.
> Completion criteria: every item below is checked in the packaged app, and any defect found is fixed here.

## Checks

- [ ] Tab strips in the packaged app (WKWebView), compared before and after the tab wrappers, as
  `docs/memory/verifying-the-desktop-ui.md` describes: no tab's icon or label shifts between pixel rows while the
  selection indicator moves, and a hovered tab does not dim or shake, in the Settings sections, the agent strips of the
  accounts section and the rail's quota panel, the shell terminal tabs, the project browser's Files / Changes tabs and
  the text strips of the dialogs. Headless WebKit confirmed each tab's computed transform and hover opacity, but cannot
  show the pixel shift.
- [ ] Buttons and toggles in the packaged app, after the move to the layer's `Button`, `BareButton`, `ToggleButton` and
  `ToggleButtonGroup`: they look as before (HeroUI's variants, the `xs` size, the chrome fills, the alert buttons'
  warning treatment, the row icon buttons), a press of a chrome or row control still leaves focus on the terminal, and
  VoiceOver reads a button under a tooltip that repeats its name once. The unit and component tests covered the
  attributes and focus hand-off, not the rendered look or the screen reader.
- [ ] Form fields in the packaged app, after the move to the layer's field wrappers: text fields (with the trailing
  browse button, the spinner adornment and the clone directory's folder prefix), search fields (the clear button, the
  file filter's incomplete-search mark), checkboxes with their notes, switches, radio groups, the option selects (the
  clear button, the marks, options with a reason) and the virtualized branch select look and behave as before, and
  VoiceOver still reads a disabled select option as unavailable and a field's description and error once. Typecheck,
  lint and the component tests covered the wiring and attributes, not the rendered look or the screen reader.
- [ ] Overlays in the packaged app, after the move to the layer's `Modal`, `Popover`, `FloatingPopover`, `Menu`,
  `Tooltip` and `Toast` wrappers: dialogs and alert dialogs (the status icon, the close button and its tooltip, the
  viewer's full-screen and inset sizes, a failure replacing a dialog's content), the settings dialog, the action menus
  (a context menu opening at the pointer, a submenu, a submenu panel, the choices' check marks, a disabled item with
  its reason), tooltips (a control's, a mark's with its touch popover, an anchored one, and one closing as a modal
  opens over its trigger), the hover popover hints, the Details popover and the project filter's popover, and the
  toasts (focus kept where it was, the close button) look and behave as before; overlays holding tabs fade in and out
  without their tabs shifting (WKWebView). Typecheck, lint, the wrappers' tests and the existing component tests
  covered the wiring, focus and attributes, not the rendered look.
- [ ] Display components in the packaged app, after the move to the layer's `Alert`, `Surface`, `Card`, `EmptyState`,
  `Badge`, `Chip`, `Tag` and `TagGroup`, `Spinner`, `ProgressBar`, `Meter`, `ProgressCircle`, `Disclosure`, `Code`,
  `Kbd` and `ScrollShadow` wrappers: error alerts and warnings, the diagnostic and code blocks, empty and failure
  panels, the welcome screen's tools card, the rail's count and status badges and the bell's dot, status badges and
  inline references in copy, the project filter's tag picker and summary, the tags field's removable tags (the remove
  button's fade and tooltip), spinners, the update strip's and account sharing's progress bars, the quota and context
  meters and the context ring, the Advanced section and option details disclosures, the key caps in menus and
  tooltips, and the scroll fades (the sticky-heading lists among them) look and behave as before; VoiceOver reads the
  loading-more spinner by its name and skips the decorative ones and the badges. Typecheck, lint, the wrappers' tests
  and the existing component tests covered the wiring and attributes, not the rendered look or the screen reader.
- [ ] Trees and focus in the packaged app, after the move to the layer's `Tree` wrappers and focus hooks: the Files
  tree and the Changes list (virtualized scrolling, the expand chevrons and their names, Left / Right and type-ahead,
  Option+Tab out of the list, a failed folder's mark), the Changes list's section headings sticking above the scroll
  fades through a long list, and the keyboard focus rings after F6, a find step, history navigation, a pane toggle and
  a failed submenu's Try again look and behave as before. Typecheck, lint, the layout test and the existing tree and
  focus tests covered the wiring and attributes, not the rendered look.
- [ ] The tags field and the session colour picker in the packaged app, after the move to the layer's `ComboBox` and
  `ColorSwatchPicker` wrappers, and the parts found through the layer's `partSelector`: the tags field (its box, the
  suggestions list kept open across picks, the trigger's tooltip, pressing the trigger of a field partly scrolled out
  of view bringing it into view with the list open) and the read-only tags of the project filter's summary; the
  Colour submenu's swatches (the ring on the current colour, the opening focus on it from the keyboard, VoiceOver
  naming each colour); and the focus hand-offs that find a part (a select's trigger after a removal or a model change,
  the Advanced section's trigger, the session model dialog's footer button, a menu's focus held in its popover, the
  toasts kept clear of a dialog's footer and focused by region navigation) look and behave as before. Typecheck, lint
  and the existing component tests covered the wiring, not the rendered look or the screen reader.
- [ ] Busy dialogs in the packaged app (WKWebView), after the move to the layer's `Modal` busy state: while a request
  runs in HandoffDialog, AddAgentDialog, SessionDialog and the Export dialog, the body's inputs, selects and switches
  and the Install link are disabled and unreachable by Tab and by the pointer, the pending button keeps keyboard focus,
  and Escape and a click outside do not close the dialog; a failed request returns focus to the field it was on once its
  error alert is acknowledged; the import's file-picker wait and a confirmation's pre-check (`waiting`) leave the dialog
  operable and closable; an Install link that has focus when Add starts leaves focus inside the dialog, on the pending
  button; a resume refusal that asks again in SessionDialog (Open disabled when its alert is acknowledged), whether
  submitted with Enter in the field or by pressing Open, ends with focus on the Session ID field. Typecheck, lint and
  the existing dialog tests covered the wiring and focus in jsdom, not WebKit's blur of a control as it becomes
  disabled.
