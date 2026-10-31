# 03 Program status reports

> Goal: a program in a session's terminal can report its own status through OSC 7501, the Program Status Protocol.
> Completion criteria:
> - A program writing OSC 7501 reports moves its session through working, waiting for the user and idle.
> - A report that breaks a limit below, or carries an unknown state, is ignored.
> - A program asking whether the protocol is supported gets an answer.

## Technical design

- [ ] Parsing OSC 7501 reports out of terminal output:
  - A report is `OSC 7501 ; key=value:key=value… ST`, where `ST` is `ESC \` or `BEL`.
  - `OSC 7501 ; ? ST` asks whether the protocol is supported, and which states and kinds are.
- [ ] The fields:
  - `state` is required: `idle`, `working`, `done`, `blocked` or `error`, or `clear`. An unknown state discards the
    whole report.
  - `id` is a `/`-separated path addressing a record. Without it, the report addresses the root record.
  - `kind` (only with `blocked`) is `permission`, `question` or `auth`.
  - `progress` (only with `working` or `blocked`) is 0 to 100.
  - `app` is a stable program identifier.
  - `title` and `msg` are base64 UTF-8 text.
- [ ] The limits:
  - The whole sequence is at most 4096 bytes, and a key at most 16 bytes.
  - `msg` is at most 2048 bytes decoded, and `title` at most 192.
  - `app` is at most 32 bytes.
  - `id` is at most 128 bytes, with at most 32 per segment and 8 levels.
  - A terminal keeps at most 256 records.
- [ ] Records by id. Each report replaces its record. `clear` removes the addressed record and its children, or every
      record without an id. The root record is the program's status.
- [ ] A turn ends when the root record leaves `working`.
- [ ] The support query, answered by the daemon.
- [ ] Mapping the reported states onto session statuses.

## Notes for the developer

- **Reusable capabilities**:
  - The daemon's continuous output parser.
  - The adapter interface through which each agent's status reaches the rest of the daemon.
- **Development notes**: builds on daemon-side answering of terminal queries, which is planned separately and has to
  exist first. Precedence over the other status sources is open (see the overview).
- **Reference docs**: `docs/product/session-statuses.md`, `docs/agent-cli-reference.md`, and the Program Status
  Protocol specification, revision 0.4 (<https://www.superlogical.com/rex/docs/build/program-status>).
