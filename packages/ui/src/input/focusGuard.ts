// Works around a react-aria defect that surfaces when a frame holds keyboard focus.
//
// While a press on a control that keeps focus where it was (`focusOnPress={false}`) is in flight, react-aria listens on
// the window (capture phase) for focus events and asks whether each event's target is, or lies inside, the control
// (`Node.contains(target)`). When keyboard focus sat inside an iframe (the report panel's page, once the user clicked
// in it), the press moves focus out of that frame and the window itself receives `focus`/`blur`/`focusin`/`focusout`
// with the Window as the event target. A Window is not a Node, so `contains` throws a TypeError out of the listener,
// the refocusing that follows never runs, and focus ends up on `<body>` — off the terminal, which must keep keyboard
// focus.
//
// The guard is a capture listener on the window, and at the target capture listeners run before non-capture ones
// whatever the order of registration, so while it is armed it silences every non-capture window listener, and the
// capture ones registered after it, react-aria's press listeners among them. It is armed from a press's `mousedown`:
// react-aria's listeners are added then and removed in its own animation frame at the latest, and the guard disarms
// one tick after that frame, so the two never end in the same frame whatever order their callbacks run in. Outside the
// armed span it stops nothing: the window's own `focus` and `blur`, as the user leaves the window for another
// application or a frame and comes back, reach every listener.
//
// It must load after react-aria's module-level focus-visible capture listener on the window, so that listener still
// runs before the guard: registered after it, it would be silenced like the rest while the guard is armed.
//
// The span can swallow a real change: a click that activates the window can send its `mousedown` before its `focus`, and
// no frame runs while the document is hidden, which keeps the guard armed until one does. So on disarming, the guard
// compares `document.hasFocus()` with what the last `focus` or `blur` it let through said, and replays the missed one
// once it is off, for whatever follows the window's focus to catch up.

let pressing = false;
let frame = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
// Whether the window had focus as of the last `focus` or `blur` the guard let through.
let heard = document.hasFocus();

function disarm() {
  pressing = false;
  if (document.hasFocus() !== heard) window.dispatchEvent(new Event(heard ? "blur" : "focus"));
}

window.addEventListener(
  "mousedown",
  () => {
    pressing = true;
    cancelAnimationFrame(frame);
    clearTimeout(timer);
    frame = requestAnimationFrame(() => {
      // A timer from within the frame, not a nested frame: it runs after every callback of this frame, react-aria's
      // cleanup included, and a pause of frames while the document is hidden is waited out once, not twice.
      timer = setTimeout(disarm, 0);
    });
  },
  true,
);

for (const type of ["focus", "blur", "focusin", "focusout"]) {
  window.addEventListener(
    type,
    (event) => {
      // Whether the target is not a Node, which is what `contains` refuses, rather than whether it is `window`: the
      // same in a browser, and also true of the window jsdom dispatches on in tests.
      if (event.target instanceof Node) return;
      if (pressing) event.stopImmediatePropagation();
      else if (type === "focus" || type === "blur") heard = type === "focus";
    },
    true,
  );
}

export {};
