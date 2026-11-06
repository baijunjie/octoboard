/** Selectors for the parts of the layer's components that code outside it has to find in the DOM: an element a focus
 * hand-off goes to, a region whose geometry is measured, a test fixture's target. Which attribute or class marks each
 * part is the library's business, so it is written here only, and a change of library changes it here. */
export const partSelector = {
  /** The popover of a `Menu` (its `MenuPopover`), submenus' included. */
  menuPopover: "[data-slot=dropdown-popover]",
  /** The visible name of a menu or list item (a `Label` inside it). */
  itemLabel: "[data-slot=label]",
  /** The button of a `Select` that opens its options (`SelectTrigger`). */
  selectTrigger: "[data-slot=select-trigger]",
  /** The clear button of a `Select` (`SelectClearButton`). */
  selectClearButton: "[data-slot=select-clear-button]",
  /** A `Disclosure`, and its trigger. */
  disclosure: "[data-slot=disclosure]",
  disclosureTrigger: "[data-slot=disclosure-trigger]",
  /** The footer of a dialog or alert dialog (`ModalFooter`). */
  dialogFooter: "[data-slot=modal-footer], [data-slot=alert-dialog-footer]",
  /** A `Button` that is `pending`. */
  pendingButton: "[data-pending=true]",
  /** What wraps a `Tooltip`'s trigger (`TooltipTrigger`); not the `phrasing` form, whose span is not marked. */
  tooltipTrigger: "[data-slot=tooltip-trigger]",
  /** The region the toasts are stacked in (`ToastRegion`), and one toast in it. */
  toastRegion: "[data-slot=toast-region]",
  toast: "[data-slot=toast]",
  /** A toast that is on screen, not one fading out or hidden behind the stack's limit. */
  liveToast: '[data-slot=toast]:not([data-exiting="true"]):not([data-hidden="true"])',
  /** A toast's text: its title and description, which can be selected. */
  toastText: "[data-slot=toast-title], [data-slot=toast-description]",
  /** The scroller of a `TabListContainer`, and the container around it. */
  tabScroller: ".tabs__list-container__scroller",
  tabListContainer: ".tabs__list-container",
  /** A `ScrollShadow`, or an element drawn as one with `useScrollShadow`. */
  scrollShadow: ".scroll-shadow",
} as const;
