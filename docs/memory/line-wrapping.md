# Line wrapping in committed text

## Format checks do not enforce prose width, so re-wrap the whole paragraph you edited

Wrap Rust code and comments at 100 columns; `rustfmt.toml` sets that width for code formatting.
Committed Markdown prose wraps at 120, while a table row and anything inside a fenced block stay on one line
however long they get.

Format checks do not enforce these widths for prose. `cargo fmt` reformats code but never re-wraps a comment, and
it leaves a string literal alone as well (`format_strings` is off), so a long description inside an embedded JSON
schema is invisible to it; ESLint's formatting rules set no maximum line length. Clean format, lint and type checks
therefore do not prove that prose meets its width, and only a reader catches an over-long line.

Rust code is the other way round: its layout is whatever `cargo fmt` produces, which the format check holds it to. A
code line rustfmt leaves past 100 columns because it cannot break it (a closure head, a long signature) is not fixed
by wrapping it by hand, and rustfmt also collapses a block body written out by hand back onto one line when it fits.
So such a line is not a review finding to answer with formatting; shorten it only by changing the code, such as
binding part of it to a local, or leave it.

An edit made in place is what produces them — a sentence added to a doc comment, a clause changed mid-paragraph
in Markdown, a rename that lengthens an identifier or term that comments and prose mention — because the line grows
while the rest of the paragraph keeps the old wrapping. So after editing a comment or any prose, re-wrap that whole
paragraph rather than only the line you touched; after a rename, that means every paragraph it changed, not only the
code.

## Count a line's width in characters, not bytes

Em dashes, arrows and CJK text are several bytes each, so a byte count reports lines as over-long that are not,
and a report built that way sends someone off to re-wrap correct prose. `wc -c` is a byte count, and so is
`awk`'s `length` in some implementations whatever the locale says (mawk, which is `awk` on Debian and Ubuntu, is
one). So measure with something that decodes UTF-8 and counts the decoded characters — `python3 -I -c` over the
lines of the file is enough — before flagging a line or re-wrapping one, including when checking a width someone
else reported.
