/** The attributes whose change takes a toast off the screen or puts it back (`liveToast` reads them), for a mutation
 * observer's `attributeFilter`. */
export const toastStateAttributes = ["data-hidden", "data-exiting"] as const;

/** Selectors for the parts of the layer's components that code outside it has to find in the DOM: an element a focus
 * hand-off goes to, a region whose geometry is measured, a test fixture's target. Which attribute or class marks each
 * part is the library's business, so it is written here only, and a change of library changes it here. */
export const partSelector = {
  /** The popover of a `Menu` (its `MenuPopover`), submenus' included. */
  menuPopover: "[data-slot=dropdown-popover]",
  /** The visible name of a menu or list item (a `Label` inside it). */
  itemLabel: "[data-slot=label]",
  /** The visible name of a field (its `Label`). */
  fieldLabel: "[data-slot=label]",
  /** The root of a time field or a date picker (`TimeField`, `DatePicker`): what holds its label, segments and note. */
  temporalField: "[data-slot=time-field], [data-slot=date-picker]",
  /** A segment of a temporal field that holds no value yet and shows its placeholder; `segmentSelector` is one by type. */
  blankSegment: "[role=spinbutton][data-placeholder=true]",
  /** The popover of a `DatePicker` with its calendar, while it is open. */
  calendarPopover: "[data-slot=date-picker-popover]",
  /** The button of a `Select` that opens its options (`SelectTrigger`). */
  selectTrigger: "[data-slot=select-trigger]",
  /** The clear button of a `Select` (`SelectClearButton`). */
  selectClearButton: "[data-slot=select-clear-button]",
  /** The layer's `Spinner`. */
  spinner: "[data-slot=spinner]",
  /** A `Disclosure`, and its trigger. */
  disclosure: "[data-slot=disclosure]",
  disclosureTrigger: "[data-slot=disclosure-trigger]",
  /** The footer of a dialog or alert dialog (`ModalFooter`). */
  dialogFooter: "[data-slot=modal-footer], [data-slot=alert-dialog-footer]",
  /** A control the library marks disabled without a native `disabled` (a menu item, a list option). */
  disabledControl: '[data-disabled="true"]',
  /** A `Button` that is `pending`. */
  pendingButton: "[data-pending=true]",
  /** What wraps the trigger of a mark `Tooltip` (`trigger="mark"`); not the `phrasing` form, whose span is not marked. */
  tooltipTrigger: "[data-slot=tooltip-trigger]",
  /** The visible box of a `Switch` (its track) or a `Checkbox`, which the hidden input behind it is not. */
  toggleBox: ".switch__control, .checkbox__control",
  /** The region the toasts are stacked in (`ToastRegion`), and one toast in it. */
  toastRegion: "[data-slot=toast-region]",
  toast: "[data-slot=toast]",
  /** A toast that is on screen, not one fading out or hidden behind the stack's limit. */
  liveToast: `[data-slot=toast]${toastStateAttributes.map((name) => `:not([${name}="true"])`).join("")}`,
  /** A tooltip that is open, not one fading out. */
  liveTooltip: "[role=tooltip]:not([data-exiting])",
  /** A toast's text: its title and description, which can be selected. */
  toastText: "[data-slot=toast-title], [data-slot=toast-description]",
  /** The scroller of a `TabListContainer`, and the container around it. */
  tabScroller: ".tabs__list-container__scroller",
  tabListContainer: ".tabs__list-container",
  /** A `ScrollShadow`, or an element drawn as one with `useScrollShadow`. */
  scrollShadow: ".scroll-shadow",
} as const;

/** The segment of a temporal field that holds `type` (`hour`, `minute`, `dayPeriod`, `year`...), or any segment with
 * no `type`. */
export const segmentSelector = (type?: string): string => (type === undefined ? "[role=spinbutton][data-type]" : `[role=spinbutton][data-type=${type}]`);
