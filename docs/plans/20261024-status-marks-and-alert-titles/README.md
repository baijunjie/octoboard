# Status marks, touch tooltips and Alert titles

## Problem

- **A title-less Alert is misaligned.** HeroUI 3's stock Alert centres its indicator on the title's 24px line; the
  description has a 20px line, so an Alert with no title draws its first line 2px above the glyph's centre. A
  per-caller line-height fix was lost once Alerts were moved back onto stock HeroUI, and the standard forbids a CSS
  exception. Title-less Alerts exist in the file viewer's notices, several dialogs, Settings, the startup screens and
  the crash screen.
- **The file viewer is a compact layout and should not use Alerts at all.** Its notices (syntax highlighting turned
  off, a file too large, a diff that could not be drawn, a file still in conflict…) are Alert strips stacked above the
  content, and blocks inside a Markdown document.
- **Status and error marks wire two floating surfaces onto one glyph.** `FailureMark` shows a tooltip repeating its
  name on hover and a popover with the reason on press; `HelpTip` shows the explanation in a tooltip on hover and in a
  popover on press, with extra logic to keep the two from stacking. The user's view: these are one component with two
  behaviours, not two surfaces.
- **Tooltips do not show on touch screens.** A tooltip answers only the pointer's hover, which a finger has not got, so
  on a phone or tablet every tooltip's text is out of reach.

## Plan outline

1. A tooltip wrapper: a tooltip with a mouse, a press-opened popover drawn like the tooltip on touch. `HelpTip` becomes
   that wrapper on a "?".
2. A status mark for errors and warnings: always a popover, which a mouse also opens by hovering. `FailureMark` and its
   callers move onto it, dropping the name-repeating tooltip.
3. The file viewer drops Alerts: a whole-file notice becomes a status mark after the file path; a notice about one spot
   in Markdown or diff content stays at that spot as the same kind of mark.
4. The UI standard gains the rules "an Alert has a title" and "an Alert holds its own details", enforced in code
   where it can be, and every Alert that breaks them is brought into line.

## Key design decisions

- **The tooltip wrapper chooses by the interaction, not by sniffing the device** (proposal). A mouse hover or keyboard
  focus opens the tooltip; a touch press opens the popover. A hybrid device (a touch-screen laptop, a tablet with a
  trackpad) gets whichever answers the input actually used. The project already tells a touch press from the rest
  (`interactionIsTouch`, `CANNOT_HOVER`).
- **Every tooltip goes through the wrapper, and a trigger that has an action of its own opens no popover on touch**
  (user direction; the mechanism is the recommendation below). The two ways considered:
  - *Recognising a button automatically* at run time is not reliable. A react-aria control takes its press handling
    from context and hides it inside its own hooks, so the wrapper cannot read from props whether the trigger acts
    (`onPress`, a menu trigger, a link, a selection) — and a DOM check for `button` / `role="button"` is wrong both
    ways: the non-control mark's own trigger is itself a `role="button"` element, while a row's action lives on a
    `row` / `treeitem` ancestor rather than on the trigger.
  - *An explicit parameter* is reliable, and its default can follow the trigger's shape, which the wrapper already
    knows statically: the control shape (a react-aria control child, today's `TitledControl`) defaults to no touch
    popover, so a tap runs the control's action; the mark shape (a non-control indicator, today's
    `IndicatorTooltip`) defaults to the touch popover. A control whose only action is to show its text (the
    "?" help tip) is a popover hint rather than a control with a touch popover. The anchored shape (today's
    `AnchoredTooltip`, driven by its caller's hover inside a row) has no trigger to tap — a tap on the row is the
    row's action — so it never opens a touch popover.
  - **Recommended: the explicit parameter with the shape-based default.** The three existing names stay as the
    wrapper's shapes over one shared core, so their call sites barely change.
- **On touch, the popover looks like the tooltip** (user decision): same surface, typography, size and arrow, so the
  two read as one component.
- **Error and warning marks are always a popover** (user decision), because their content can hold a Try again button
  and raw text, which a tooltip cannot. With a mouse, hovering the mark opens the popover too. A hover-opened popover
  does not move focus; it stays open while the pointer is on the mark or inside the popover and closes shortly after
  the pointer leaves both, so its buttons can be reached. A mouse click opens nothing (user decision: where the pointer
  can hover, the hover is what opens it); a tap, a pen, the keyboard or assistive technology opens it with focus moved
  into it. The rail quota panel follows the same rules and is the same component.
- **A mark of every tone is the same popover mark** (user decision): a note (informational tone) is the same button
  and popover as an error or a warning mark, so marks side by side look, take the pointer and open alike. Each tone
  keeps its own glyph shape — circled `i`, triangle, circled `!` — so tones differ by shape, not only colour. The "?"
  help tip is the same kind of popover (user decision), so its explanation can be selected with the mouse.
- **A whole-file notice goes after the file path in the viewer header** (user decision); one mark per notice, ordered
  failed, then warned, then notes (user decision). A notice
  that appears after the viewer has painted is still spoken through the existing status announcement, and the mark
  carries an accessible name.
- **An Alert must have a title** (user decision) — this is the rule that keeps the misalignment from coming back, in
  place of a CSS exception. It is enforced in code so a new caller cannot omit it. "A short error that already says
  what happened needs no generic title" in the standard is withdrawn.
- **An Alert holds its title and its own details; only the remedy sits after it** (user decision). The earlier rule
  "an inline Alert holds only what failed and why; the remedy sits after it" was meant to move only the remedy out,
  but combined with "stock structure only — `Alert.Description` cannot hold a list" it pushed an Alert's own details
  out too: the delete-session confirmation shows its warning's title in the Alert and the list of sessions deleted
  along with it outside, followed by consequence lines that read as the Alert's continuation. Details go inside
  `Alert.Content`, after the title and description, in the stock text style.

## Milestones

1. 01 Tooltip wrapper that works on touch (closed)
2. 02 Error and warning status marks (closed)
3. 03 File viewer notices become status marks (closed)
4. [04 Every Alert has a title and holds its own details](04-alert-titles.md)
5. [05 Final confirmation](05-final-confirmation.md)

Milestone 04 depends on none of the others but runs after 03, which removes the
viewer's Alert-based notices so 04 need not convert them.
