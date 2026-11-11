# Finding in the file viewer

**Find in file** searches the text the file viewer shows (see `docs/product/file-viewer.md`) and highlights what it
finds. **⌘F** (Ctrl+F off macOS), pressed anywhere in the viewer, opens the **find bar** with keyboard focus in its
query, and so does the header's Find in file control (see "The header" in `docs/product/file-viewer.md`). The key is
matched by its physical key, so a keyboard layout does not move it, and it cannot be changed (see "What is listed" in
`docs/product/keyboard-shortcuts.md`). ⌘F fills the query with text selected wholly within one searched view, even
when the bar is already open, and searches immediately; carriage returns and line feeds are removed, and other
whitespace is kept. It focuses and selects the query, keeping the current query when no searchable text is selected
and keeping the switches in either case. Closing the bar keeps its query and switches while the viewer stays open,
so the bar opens again with them.

## What is searched

A file's code, highlighted or shown as plain text, a Markdown file's source, a rendered Markdown document together with
the code of its fenced blocks (see `docs/product/markdown-documents.md`), a change's diff in either layout, and a plain
patch. The rest of what the viewer draws — the header, the notices, a diagram's drawing — is not searched, nor are a
diff's collapsed lines until they are expanded (see "Expanding the collapsed lines" in
`docs/product/project-pane-git-change-viewer.md`). In highlighted code and in a drawn diff a match lies within one line,
and in a rendered document within one block — a paragraph, a heading, a list item, a table cell — never running from
one into the next.

An image shown as an image, a binary file, a stand-in and an error have nothing to search (see "Notices and stand-ins"
in `docs/product/file-viewer.md`): there the header offers no Find in file and ⌘F does not open the bar.

## The find bar

The bar floats over the top trailing corner of the content and stays on one row down to a 320 px window width. While it
is open, each searched view starts its text below the bar, so the bar covers no line until the text is scrolled under
it; opening or closing it over scrolled text leaves that text where it was. In order, it holds:

- **The query**, a field with the placeholder "Search". It is searched as typed, a space at either
  end included, once typing pauses.
- **The count**: the current match's number and the number of matches, written short — "1/13", or "0/0" when nothing
  matches — and "Invalid", in the failure colour, for a regular expression that does not compile; nothing while the
  query is empty. At most 5,000 matches are found, and past that the count reads as "1/5,000+". What assistive
  technology is told, as the count changes, is the full sentence instead: "1 of 13", "1 of 5,000+", "No matches",
  "Invalid regular expression".
- **Match case**, **Whole word** and **Use regular expression**, three switches, all off when the viewer opens. Without
  Match case, upper and lower case are not told apart. Whole word takes only a match that no letter, digit or underscore
  stands right before or after, letters of any script included. With Use regular expression the query is a JavaScript
  regular expression, in which `^` and `$` match at the start and end of each line; a match of no characters is not
  counted. Each switch keeps one icon and one name, which is also its tooltip, and shows that it is on by a fill and an
  outline; pressing one leaves keyboard focus in the query.
- **Previous match** and **Next match**, disabled while there is no match. **Enter** in the query steps to the next
  match and **Shift+Enter** to the previous one; both go round from the last match to the first and back. Enter
  pressed before typing pauses searches at once. Neither button takes keyboard focus from the query.
- **Close find**.

## The matches

Every match is highlighted, and the current one more strongly, with a text colour of its own and an underline, so it is
told from the others by more than colour. Typing, filling the query from selected text, changing a switch and moving
to another file or change make the first match current; stepping makes the next or previous one current. Each of these
also scrolls the current match to the middle of the view, and sideways into view on a line that does not wrap; nothing
else scrolls the text.

When what is drawn changes — wrapping turned on or off, the diff's layout switched, the file read again after a change
on disk, collapsed lines expanded — the text is searched again, without scrolling, and the current match keeps its
number. Where the browser cannot paint highlights, the count and the scrolling still work.

## Moving to another file or change

Moving to another file or change (see "Moving between files" in `docs/product/file-viewer.md`) keeps the bar, its query
and its switches; the new subject is searched once it is drawn, and its first match is current. While a subject is being
read the bar stays, with no count. Over a subject with nothing to search the bar is hidden, keyboard focus in it moving
to the viewer, and it comes back with the next subject that has text.

## Closing the find bar

**Escape**, Close find or the header's Find in file control, whose name reads "Close find" while the bar is open,
closes the bar. While the bar is open Escape is the bar's wherever keyboard focus is in the viewer, and the viewer
itself does not close on it — the next Escape does (see "Closing it" in `docs/product/file-viewer.md`) — except that a
control whose popup is open keeps the key, and that with a tooltip showing the first Escape only takes the tooltip
away. Keyboard focus that was in the bar goes to the searched text after Escape or Close find, and back to the header's
control when that closed it.
