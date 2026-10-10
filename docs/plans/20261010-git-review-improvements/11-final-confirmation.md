# Final confirmation

> Goal: the checks that could not be made while the work was being done are made, and anything they turn up is fixed
> here.
> Completion criteria: every item below is confirmed in the packaged app, or fixed and then confirmed; nothing is
> left beyond this milestone.

## Carried from milestone 02 (expanding collapsed lines in the viewer)

The expansion was verified in the packaged app for both views, both layouts, the failure and moved-on states, the
keyboard walk and the reconnect. These four could not be settled then:

- [ ] A screen reader (VoiceOver) on the separators: their names, the loading and failure announcements, and the
  controls marked unavailable while a read is out. This needs a person.
- [ ] A real input method typing into the viewer while a diff is shown, confirming composition does not reach the
  separator controls' key handling. This needs a person.
- [ ] Right-to-left with text that is actually right-to-left. The run so far used mirrored English, as no Arabic
  catalog exists; confirm against a language the app has, or against the mirrored layout with right-to-left content
  in the file itself.
- [ ] A window below 800 px. "Show whole file" is positioned absolutely and may cover the end of a long separator
  label. The macOS window's minimum is 1148 px, so this one is reached in the gallery rather than the packaged app.

## Carried from milestone 03 (word wrap in the viewer)

The wrap choice was verified in headless WebKit over the gallery and in unit tests; the machine's screen was locked
throughout, so the packaged app never showed it:

- [ ] The wrap toggle in the viewer header, with the header's rows unchanged by it.
- [ ] Wrapping on for a file and for a diff in both layouts, the split layout's two sides staying level.
- [ ] The choice surviving a real restart of the app, not only a written preference.
- [ ] A diff with wrapping off, scrolled sideways: its separator controls still usable and not clipped.

## Carried from milestone 04 (the change list as a directory tree)

Verified in headless WebKit over the gallery and in unit tests; the machine's screen locked before every attempt at
the packaged app, so none of this was seen in the real window:

- [ ] The Group by folder toggle switching flat to tree and back, with the tab row's layout intact at the pane's
  normal width, at the aside's 300 px minimum and under a right-to-left language. The toggle is laid over the tab
  row absolutely, and at 300 px the tab list has about 244 px, where `Tabs.ListContainer`'s own scroll chevrons
  would sit under it.
- [ ] The tree under the existing sections: counts unchanged, directories before changes, a single-child chain as
  one row.
- [ ] Folding and unfolding by pointer and by Left/Right, and a fold surviving a switch between the two views.
- [ ] Previous/Next following the rows as shown, skipping changes under a folded directory, and a change open when
  its directory is folded keeping its place.
- [ ] The grouping choice surviving a real restart, and the folded directories not surviving it.

## Notes for the developer

**Reference docs**

- `docs/memory/building-and-launching-the-app-for-verification.md`, `docs/memory/verifying-the-desktop-ui.md`
