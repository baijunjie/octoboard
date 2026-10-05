// Works around a react-aria defect that surfaces when a frame holds keyboard focus.
//
// While a press on a control with `preventFocusOnPress` is in flight, react-aria listens on the
// window (capture phase) for focus events and asks whether each event's target is, or lies inside,
// the control (`Node.contains(target)`). When keyboard focus sat inside an iframe (the report
// panel's page, once the user clicked in it), the press moves focus out of that frame and the
// window itself receives `focus`/`blur`/`focusin`/`focusout` with the Window as the event target.
// A Window is not a Node, so `contains` throws a TypeError out of the listener, the refocusing
// that follows never runs, and focus ends up on `<body>` — off the terminal, which must keep
// keyboard focus.
//
// A listener registered before react-aria's, which are added per press, swallows exactly those
// window-targeted events. Nothing here relies on them: react-aria's focus-visible tracking
// registers its own window listeners at module load (`main.tsx` imports the App, and react-aria
// with it, before this file, so they are registered first and still run), and the only event it loses
// is the window `blur`, whose state the next window `focus` re-arms. Every other focus listener in
// the UI is on an element.

for (const type of ["focus", "blur", "focusin", "focusout"]) {
  window.addEventListener(
    type,
    (event) => {
      if (event.target === window) event.stopImmediatePropagation();
    },
    true,
  );
}
