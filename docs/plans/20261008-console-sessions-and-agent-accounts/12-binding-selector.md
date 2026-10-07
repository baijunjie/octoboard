# Choosing a binding when a session is created

> Goal: the user chooses, when starting a session by hand, which console session it is bound to, or that it is bound
> to none.
> Completion criteria: in a console with two console sessions, the new-session dialog offers both and "none",
> defaults to none, and the session created is bound to whatever was chosen; in a console with none, no choice is
> offered and the session is unbound. The choice cannot be changed afterwards. Static checks and the test suite pass.

## Technical design

- [ ] The new-session dialog's membership checkbox becomes a **choice of owner**: one of the console's console
  sessions that are not archived, or none.
- [ ] The default is **none** — a session the user starts stays outside any orchestration unless they say otherwise.
- [ ] A console with no console session that is not archived shows no choice at all, and the session is unbound.
- [ ] Each option shows the console session's colour alongside its name, the same colour as the binding badge, so the
  choice and the badge read as the same thing.
- [ ] The choice is fixed for the session's lifetime, as the membership checkbox was.
- [ ] What the two focus modes do to this choice — force it, or remove it — is in the next milestone.

## Implementation plan

- [ ] Replace the checkbox with the owner choice in the dialog and in the creation request it builds.
- [ ] Carry the chosen owner through session creation to the stored binding.
- [ ] Add the strings for every locale, following the project's copy conventions.

## Notes for the developer

**Reusable capabilities**

- The dialog already has the field, validation and error-under-the-field conventions this choice follows.

**Development notes**

- The creation request is part of the daemon protocol; the field replacing the boolean is a protocol change.

**Reference docs**

- `docs/product/sessions.md` for the new-session dialog's fields
- `docs/product/launching-agents.md` for how a session is opened
