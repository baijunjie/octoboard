# Line wrapping in committed text

## Nothing checks line width, so re-wrap the whole paragraph you edited

Rust wraps at 100 columns, comments included (`cargo fmt`'s default; the repository sets no `rustfmt.toml`).
Committed Markdown prose wraps at 120, while a table row and anything inside a fenced block stay on one line
however long they get.

No check enforces either. `cargo fmt` reformats code but never re-wraps a comment, and it leaves a string
literal alone as well (`format_strings` is off), so a long description inside an embedded JSON schema is
invisible to it; `packages/ui` has no formatter at all. A clean `cargo fmt --check` and a clean typecheck
therefore say nothing about width, and only a reader catches an over-long line.

An edit made in place is what produces them — a sentence added to a doc comment, a clause changed mid-paragraph
in Markdown — because the line grows while the rest of the paragraph keeps the old wrapping. So after editing a
comment or any prose, re-wrap that whole paragraph rather than only the line you touched.

## Count a line's width in characters, not bytes

Em dashes, arrows and CJK text are several bytes each, so a byte count reports lines as over-long that are not,
and a report built that way sends someone off to re-wrap correct prose. `wc -c` is a byte count, and so is
`awk`'s `length` in some implementations whatever the locale says (mawk, which is `awk` on Debian and Ubuntu, is
one). So measure with something that decodes UTF-8 and counts the decoded characters — `python3 -I -c` over the
lines of the file is enough — before flagging a line or re-wrapping one, including when checking a width someone
else reported.
