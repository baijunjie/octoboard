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
  label.

## Carried from milestone 03 (word wrap in the viewer)

The wrap choice was verified in headless WebKit over the gallery and in unit tests; the machine's screen was locked
throughout, so the packaged app never showed it:

- [ ] The wrap toggle in the viewer header, with the header's rows unchanged by it.
- [ ] Wrapping on for a file and for a diff in both layouts, the split layout's two sides staying level.
- [ ] The choice surviving a real restart of the app, not only a written preference.
- [ ] A diff with wrapping off, scrolled sideways: its separator controls still usable and not clipped.

## Notes for the developer

**Reference docs**

- `docs/memory/building-and-launching-the-app-for-verification.md`, `docs/memory/verifying-the-desktop-ui.md`
