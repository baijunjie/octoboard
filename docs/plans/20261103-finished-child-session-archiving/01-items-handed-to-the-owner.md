# 01 Items handed to the owner in a report

> Goal: a reporter can name items that are its owner's to carry out without those items keeping it from being archived.
> Completion criteria: a `done` report whose only items are owner items is delivered with them listed and archives the
> reporter as a `done` report with no open items does; a `done` report with any `open_items` still leaves the reporter
> running; a report that passes neither list behaves exactly as before; covered by daemon tests, and the tool
> description, `PROTOCOL.md` and `docs/product/reporting-to-owners.md` state the new argument.

## Technical design

- [ ] `report` gains one optional argument: a list of strings naming items handed to the owner (name open, see the
      overview). `summary`, `status` and `open_items` keep their names, types and meaning.
- [ ] The automatic-archiving rule reads "a report with `status: done` and no open items" as before; owner items are not
      open items for it. Every other rule in "Automatic archiving" (locks, the re-check after acceptance, an unsent
      report archiving nothing, the coordinator's unfinished-session refusal) applies unchanged.
- [ ] The report written into the owner lists owner items apart from open items, so the owner sees which are its own to
      do.
- [ ] A report kept for an interrupted owner, and a report delivered to the session that took a handed-off owner's
      place, carry the owner items like the other lines.
- [ ] A synthesised report has no owner items, as it has no open items.

## Implementation plan

- [ ] Add the argument to the tool's input schema and to the report the daemon builds, leaving callers that omit it on
      the current path.
- [ ] Render the owner items in the delivered report text next to the open items.
- [ ] Tests: `done` with owner items only archives; `done` with both lists does not; omitted argument unchanged;
      kept-report rendering includes the owner items.
- [ ] Update the tool description and the role descriptions that tell a session how to report, so a reporter knows to
      put "for the owner to do" items in the new list rather than in `open_items`.
- [ ] State the addition in `apps/daemon/PROTOCOL.md` as an additive change to a frozen tool (no rename, no change to
      the call shape) and in the `report` table of `docs/product/reporting-to-owners.md`.

## Notes for the developer

- **Reusable capabilities**: the reporting and automatic-archiving path in the daemon's messaging layer, including the
  archive re-check after acceptance; the owner-notice text helpers kept in one place in the messaging layer.
- **Development notes**: the MCP tool names and call shape are frozen; an optional argument is the only acceptable
  change. Role descriptions name tools qualified as the receiving agent shows them.
- **Reference docs**: `docs/product/reporting-to-owners.md`, `apps/daemon/PROTOCOL.md` ("The MCP server as a
  contract", "The tools of each role"), `docs/product/hub-orchestration.md`.
