# Zooming images in the file viewer

An image the file viewer shows as an image (see "What it shows" in `docs/product/file-viewer.md`) opens **fitted** —
scaled down so all of it fits the view, never scaled above its actual size, so an image smaller than the view opens at
100% — and can then be zoomed and panned, as in a system image viewer. This holds wherever the viewer is opened from,
the terminal included. How the two sides of an image change zoom together is in "An image change's two sides" below.

## Images that cannot be zoomed

- **An SVG with no size of its own** (one with only a `viewBox`) is shown fitted and cannot be zoomed: it has no zoom
  bar, and none of the ways of zooming below act on it.
- Until an image has loaded, it has no zoom bar.
- An image shown as its text, or one that cannot be decoded, is not an image on screen and has nothing to zoom (see
  "Notices and stand-ins" in `docs/product/file-viewer.md`).

## The zoom bar

A small bar floats over the image area's top trailing corner — the top right, or the top left under a right-to-left
language. Assistive technology is told it is a group named "Zoom". It holds, in order:

- **Zoom out**, disabled at the fitted scale;
- **the current percentage**, in the format of the UI's language and without decimals; it is shown only, not spoken,
  since a change made with the bar or the keys is announced on its own (see "Announcements and keyboard focus" below);
- **Zoom in**, disabled at the largest scale;
- **the fit button**, whose name and icon say what pressing it does: **Actual size** while the image is fitted, which
  draws it at 100%, and **Zoom to fit** once it is zoomed, which fits it again. While fitted it is disabled when the
  image already fits at 100%, there being no other size to go to.

Each of the three buttons is an icon button whose tooltip is its name (see "Tooltips on icon-only controls" in
`docs/product/labels-and-tooltips.md`).

## Ways to zoom

- **The keys**, anywhere in the viewer, wherever keyboard focus is inside it: **⌘+** or **⌘=** zooms in, **⌘−** zooms
  out, and **⌘0** goes back to fitted (Ctrl instead of ⌘ off macOS). Shift may be held, as `+` needs it on many layouts;
  with Alt held they do nothing. They step as the bar's buttons do. Actual size has no key: it is the fit button and a
  double-click. These keys are fixed: they cannot be changed and are not listed in Settings' Keyboard shortcuts section.
  They do not clash with the session terminal font size shortcuts, which act only while a session terminal has keyboard
  focus. In a browser, while the viewer shows an image that can be zoomed, they zoom the image and not the page;
  otherwise they are the browser's as usual.
- **⌘ or Ctrl with the wheel, and a trackpad pinch**, over the image's area, zoom continuously, keeping the point of the
  image under the pointer where it is. In a browser they zoom the image, not the page. A wheel turned without a
  modifier scrolls, as it does anywhere else in the viewer.
- **Two fingers on a touch screen** pinch-zoom the image, and moving them together pans it.
- **A double-click** goes between fitted and actual size, keeping the point under the pointer where it is: from fitted
  to 100%, and from any other zoom back to fitted. On an image that already fits at 100% a double-click while fitted
  does nothing.

The buttons and the keys keep the centre of the view where it is.

## Zoom levels and bounds

- **The smallest scale is the fitted one**: an image is never zoomed out past fitting the view.
- **The largest is 3200%**, or less for a large image, so its longest side is never drawn longer than 65,536 pixels,
  but never less than 100%: every image can be shown at its actual size.
- **Zoom in and Zoom out step through fixed levels**: 10, 25, 33, 50, 67, 75, 100, 125, 150, 200, 300, 400, 600, 800,
  1200, 1600, 2400 and 3200%, with the fitted scale and the largest scale as stops too, wherever they fall. A stop less
  than 1.1 times away from the current scale is passed over, so every press visibly zooms: a fitted 31% steps in to 50%,
  not to 33%. A step never goes past either bound.
- The wheel, a pinch and two fingers zoom to any scale between the bounds.

While the image is fitted it keeps fitting as the view is resized. A zoom chosen by the user is kept as the view is
resized, until the view grows so far that the fitted scale passes it; the image is then fitted again.

## Panning a zoomed image

An image smaller than the view is centred in it. **An image larger than the view** — in either direction — is panned:

- by scrolling, with the wheel, a trackpad or the scrollbars;
- by dragging it with the primary button, under a grab cursor; a press on a scrollbar is the scrollbar's, not a drag.
  On a touch screen one finger drags it.

While it is larger than the view, the image's area is also **a tab stop**: a region named by the file, which shows a
focus ring when reached with Tab and is scrolled from the keyboard. **Left and Right still move between files** (see
"Moving between files" in `docs/product/file-viewer.md`) rather than scrolling it, so it is scrolled sideways with the
pointer. Once the image fits the view again it stops being a tab stop. The image area is always laid out left to right,
under a right-to-left language too, since an image has no reading direction.

## How a zoomed image is drawn

Drawn **above its actual size**, an image's pixels are drawn as sharp squares rather than smoothed, so the single
pixels of a screenshot can be told apart. At or below its actual size it is smoothed as usual. It is drawn on the same
checkerboard that shows its transparent areas, whatever the zoom.

## Announcements and keyboard focus

- **A zoom made with the bar's buttons, the keys or a double-click is announced** to assistive technology as the new
  percentage ("Zoomed to 150%"). One that leaves the zoom where it was announces nothing. Zooming with the wheel, a
  pinch or two fingers is not announced.
- **A zoom that takes away the control holding keyboard focus hands focus to the zoom bar**, to its first enabled
  button, so the viewer's keys keep working: Zoom in disabled at the largest scale, Zoom out or the fit button disabled
  back at the fitted scale, or an image area that stops being a tab stop because the image fits again. When focus moves
  this way, the announcement comes after the focus move has been announced.

## When the zoom is reset

- **Opening another file opens it fitted**, whether by Previous file and Next file, the arrow keys, or opening the
  viewer again.
- **A file whose content changes while it is shown** — read again after a change on disk (see "When the open file
  changes" in `docs/product/file-viewer.md`) — keeps its zoom where it can: the zoom is held within the bounds of the
  new image, and the image is fitted again where its fitted scale passes the zoom.

## An image change's two sides

An image change in the Git mode shows its Before and After images side by side or one above the other (see "What it
shows" in `docs/product/project-pane-git-change-viewer.md`). **The two sides zoom and scroll as one**:

- **There is one zoom bar**, over the After side; over the Before side when the After side has no image on screen, as
  for a deleted file. Its percentage, the actual size its fit button goes to, and the steps of Zoom in and Zoom out are the
  After side's; Zoom in stops where either side reaches its own largest scale.
- **Each side is drawn at the After side's scale times the ratio of its own fitted scale to the After side's**, so each
  keeps the same proportion to its own view: fitted, both fit.
- **Scrolling or dragging either side scrolls the other** to the same fraction of its own scroll range, so two images
  of the same size line up pixel for pixel and images of different sizes reach their edges together. Only the
  direction that was scrolled follows. Along a direction the moved side cannot scroll, its image being centred there,
  the other side goes to the middle of its range.
- **The keys, the wheel, a pinch, two fingers and a double-click zoom both sides**, over either side and with keyboard
  focus on either side; a key is taken once for the whole change. A zoom with the pointer keeps the point under it
  where it is on the side it is over, and brings the other side to the same place.
- Each side is a tab stop of its own while its image is larger than its view, named by the file, its side being named
  by its heading.
- **Opening another change opens it fitted.** A change whose content changes while it is shown (see "When the open
  change changes" in `docs/product/project-pane-git-change-viewer.md`) keeps its zoom where it can, as a file does.
