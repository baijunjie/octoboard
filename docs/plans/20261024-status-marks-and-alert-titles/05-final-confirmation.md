# 05 Final confirmation

> Goal: confirm what could not be verified when its milestone closed.
> Completion criteria: every item below checked on the named device, any defect found fixed on the spot.

- [ ] On a real touch device (a phone or tablet, and a touch-screen laptop driven by both finger and mouse): a tap on a
  mark that opens the touch popover (the top bar's status glyph, the focus header's branch badge) and on a "?" help tip
  opens the tooltip-styled popover; an outside tap and Escape close it; a tap on an icon-only button or a sidebar row
  runs its action with no popover; the 24×24 px grown hit area of a mark does not steal hover or taps from what is
  around it (the focus header branch badge's marquee still runs on hover).
- [ ] Status marks and the rail quota panel (`Hint` of kind `popover`) in Safari / WebKit and with VoiceOver: a resting
  mouse opens the popover without moving focus and VoiceOver can reach it; Enter on a mark opens it with focus inside;
  Escape with focus in a live agent terminal and in a shell terminal goes to the terminal while a hover-opened popover
  is up; on a touch device a tap opens the press form and a mouse click on a hybrid device opens nothing.
- [ ] File viewer marks on a real touch device and in Safari / WebKit: a tap on a mark after the file path, beside a
  compared image side's heading and inside a Markdown paragraph (a note, a warning, a failure) opens its popover; a
  tap on the title's Conflicted chip opens its conflict kind; the block marks beside a diagram's and a display
  formula's fallback source line up with its first line.
- [ ] Text in a hover-opened popover (a status mark, the "?" help tip, the rail quota panel) can be drag-selected and
  copied with the mouse in the desktop app's WebKit view, inside the file viewer and Settings as well as outside a
  dialog; a mouse press on the popover's text draws no focus outline on it.
- [ ] VoiceOver in the file viewer: a notice present at first paint is not spoken; one arriving after paint (an SVG
  that falls back to its text) is spoken once, and its status region is empty again about 5 s later, so reading line
  by line meets the sentence only on its mark; a conflicted file's heading is read with its conflict kind
  ("merge.md: Conflicted. Both modified."); a conflicted file whose read failed is announced once, as an alert.
