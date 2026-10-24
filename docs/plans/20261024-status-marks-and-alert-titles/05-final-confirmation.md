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
