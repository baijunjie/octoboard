# 04 Final confirmation

> Goal: what the earlier milestones finished but could not be watched happening is watched, so the topic
> ends with nothing taken on trust.
> Completion criteria: every item below has been looked at in the running application, and whatever it
> turned up is either fixed here or written into the docs that keep it.

## Handoff

From 01, where the implementation is complete and only the looking is missing:

- [ ] **Blocked: needs the user, whose machine draws no scrollbar that takes space. The fade dims a
      scrollbar that sits along the bottom edge, and dims the inner edge of the one down the right.**
      HeroUI's mask reserves an opaque strip along the right edge only, 10 px wide
      (`--scroll-shadow-scrollbar-size`), with nothing along the bottom. Two things follow, and one look
      settles both:
      - a scrollbar along the bottom lies inside the fade's bottom 24 px for its whole length but its
        last 10 px, and
      - a classic macOS scrollbar is 15 px, wider than that 10 px reservation, so the inner 5 px of the
        vertical scrollbar fades too — this one reaches *every* faded surface, not only the ones that
        scroll both ways.
      Measured in Playwright's WebKit on a control page whose scrollbars were forced to take space: the
      mask does dim them, to between 3 % and 53 % opacity across the bottom band, and 5–17 % over the
      vertical scrollbar's inner 3 px. What could not be reached there is the real thing — macOS native
      space-taking scrollbars cannot be forced on in headless WebKit (`scrollbar-width: thin` does not
      produce them, and the WebKit bundle ignores `AppleShowScrollBars`), and overlay scrollbars, which
      is what this machine is set to, appear to be painted outside the masked layer and so show nothing
      to measure. Headless WebKit is not WKWebView for painting either.
      **How to settle it**, with System Settings → Appearance → Show scroll bars → **Always**:
      1. In the gallery (`pnpm --filter ./packages/ui gallery`), open **File viewer → Files** and pick
         **`logs/server.log`**. Narrow the window to about 420 px so its lines no longer fit, then scroll
         a little down *and* a little right. Look at the bottom edge of the dark text panel, and at the
         right edge.
      2. In the real application at its 1148×600 minimum, open a file with lines far wider than the
         panel — a minified `.js`, a long-lined log — with the viewer header's wrap toggle off, and look
         at the same two edges. The step is needed because no fixture overflows sideways at a width the
         application allows.
      The outcome is either that both scrollbars are as solid as anywhere else, which closes this, or
      that the bottom one is washed out — ghostly at the very bottom, firming up towards its top, solid
      only in the last 10 px of its length — and the vertical one's inner edge with it.
      **If it is washed out**, the surfaces to fix are the two that are themselves the horizontal
      scroller: the file viewer's plain-text surface (`PlainText` in
      `packages/ui/src/viewer/CodeSurface.tsx`) and its rendered Markdown document
      (`packages/ui/src/viewer/MarkdownSurface.tsx`). The rendered-code frame and the rendered diff are
      not among them — the renderer's own `<diffs-container>` inside them keeps the sideways scrolling,
      and the faded element itself measured `scrollWidth === clientWidth` on all 46 viewer surfaces at
      both 1440 and 1148 px — and neither is the crash screen's stack block, which stays inside its
      `max-w-[70ch]`. The fix belongs with the shared capability rather than at those two sites: a third
      mask layer sized `100% var(--scroll-shadow-scrollbar-size)` at `left bottom` mirrors the right-edge
      reservation, and the project can raise `--scroll-shadow-scrollbar-size` on its own scrollers to the
      15 px a classic scrollbar really takes without touching the library.
      **The gallery has no state in which a faded surface overflows both ways at a width the application
      allows**, which is why step 2 needs a real file; worth handing to the gallery's own skill either
      way.
