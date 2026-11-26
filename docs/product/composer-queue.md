# The composer queue

Each session that is not archived has a queue of the user's messages waiting to be sent into it. The daemon keeps it,
stores it and sends from it, and every client of the daemon sees the same queue — the desktop application's windows,
remote windows and browsers alike — so a message queued in one is seen, managed and sent in all of them, and a message
set to go once the agent is idle goes while no window is open. Messages are put on it from the message composer (see
`docs/product/message-composer.md`); the text a person is still writing is not on it, and stays in the client that
edits it.

A message leaves the queue through the session's own message delivery, exactly as a message the user sends does, so its
holds, draft preservation, reshaping and notices all apply to it unchanged (see `docs/product/message-delivery.md` and
`docs/product/slash-commands-and-image-messages.md`); the queue sits in front of it and adds no other way of writing
into a terminal.

A daemon that predates the queue sends none and refuses its requests with `unreadable_request`; a client connected to
one shows neither the composer nor a queue.

## Send modes

Every session has a **default send mode** for new messages, which the composer's When to send selector shows and sets:

| Mode | In the composer | What a new message does |
|---|---|---|
| `now` | Send now | goes to the session's message delivery at once (see "Sending a queued message now" below) |
| `when_idle` | When idle | joins the end of the queue and is sent by the daemon once the session is idle |
| `manual` | Manual | joins the end of the queue and waits there until the user sends it |

A session's default starts as `now`, is stored with the session and is the same in every client. A message waiting on
the queue is either `when_idle` or `manual`, which can be changed at any time; sending now is an action, never a mode a
message waits in.

**When idle**: once the session is idle (paused included), nothing holds its writes, and it waits on nobody, the daemon
sends the first `when_idle` message in queue order, and only that one: the next goes at the next idle. `manual`
messages are skipped and wait in place. A `when_idle` message never resumes a session: while the session has no
process it waits, and it goes once the session is resumed and idle. After any slash command from the queue is written
— sent now as well as sent by the daemon at an idle — which may start no turn (or one reported a moment late), the next
`when_idle` message waits until a turn has shown and the session is idle again, or until the session still reads idle
**5 seconds** after the command was written.

## Sending a queued message now

**Send now**, on a queued message or with the composer's Send now mode, hands the message to the session's message
delivery at once, whatever its mode and whatever the session's status, under delivery's own holds. A session with no
process is resumed (see `docs/product/archiving-and-resuming-sessions.md`), and the message is handed over once the
session can take one; a resumed session that ends again before it takes the message puts the message back, marked
**Refused** (the session is not running), with its mode kept. A send now refused at the hand-over — the session waits on
the user, say — leaves the message in its place, marked Refused with the reason and its mode kept, and raises an error
alert saying the message could not be sent.

While a message is being handed over it shows as **Sending**: it can no longer be edited or have its mode changed, and
it can be withdrawn only while it waits for the session it resumes, or while delivery holds it queued and has not
written it; not while it is being handed over or written. Once written it leaves the queue. A message delivery held and
then dropped unwritten — the session's process ended, or a relaunch or a handoff took delivery's queue — comes back as
it was, waiting, with no mark and its mode kept.

## Messages that come back

A message the queue handed over is never lost silently, and never sent twice by itself:

- **Not delivered.** Where delivery reports the message as not carried out — the notices `message_not_taken`,
  `message_not_sent`, `message_not_delivered`, `message_diverted_by_view`, `message_cleared_after_cancel`,
  `message_given_back_unsent`, `message_dropped_at_cancel` and `message_maybe_dropped_at_cancel` about it, or
  `queued_messages_dropped` for one written only in part — the message comes back to its place in the queue, switched
  to Manual and marked **Not delivered** with that notice's reason. It is never sent again by itself, since the agent
  may have taken it after all; where the notice is later withdrawn because the agent reported taking it, a message still
  as it came back leaves the queue again.
- **Refused by delivery.** A message delivery refuses once it holds it (`message_not_written`, `message_refused`,
  `command_not_recognised`, `command_not_found`), a send now included, comes back the same way, switched to Manual and
  marked **Refused**.
- **Refused at the daemon's own send.** A message the daemon was sending at an idle that is refused stays waiting in its
  mode where the refusal may pass — the session waits on the user, shows its folder-trust confirmation or has no
  process, or the daemon is stopping — and is otherwise switched to Manual and marked Refused.

Editing a message, changing its mode or sending it clears its mark.

## Files of queued messages

The images and files a message names that were saved for the session — pasted images and uploaded files (see
`docs/product/pasting-images.md` and `docs/product/attaching-files.md`) — are moved, when the message is queued, into
the session's composer directory, `~/.octoboard/composer/<session id>/` on the daemon's host, readable by its owner
alone, and the message carries their new paths; so they outlive the session's process, whose scratch directory goes
when it ends. The files are in place before the queue shows the message, and are moved back where they were when the
request is refused. A file another request has moved already, or a path named more than once within or across a
request's `images` and `files`, refuses the request (`unreadable_request`, `field` `images` or `files`, the list
where the path is found again). A file named by a path of its own, such as one dropped in the desktop application's
main window, is not moved. For a session with no process, a pasted image or an uploaded file is saved straight into
the composer directory, so a message can be put together and queued for an interrupted session.

- **Withdrawing a message, or an edit that drops a file from it**, deletes only the files that message's own queueing
  or edit moved in, and none of a message that was written already (one back as Not delivered), whose files the agent
  may still be reading.
- **The files of a sent message stay**, since the agent reads a file named by path whenever it gets to it; the
  directory goes when the session is deleted (see `~/.octoboard/composer/<session id>/` in
  `docs/product/files-octoboard-owns.md`).

## Lifetime of the queue

- **The queue and the default send mode are stored with the session**: they survive a daemon restart and the session's
  interruption. A message being sent at a restart comes back waiting, and `when_idle` messages go again once the session
  is resumed and idle. At a planned stop, a queued message delivery has not written yet comes back to the queue rather
  than being carried to the next daemon as text.
- **Archiving the session drops its queue**, and hands each message's content back in a notice of its own
  (`queued_message_discarded`), shown as an error alert titled that a queued message was dropped, saying it was dropped
  because the session was archived, with the message's text, followed by the paths of its images and files one to a
  line, under it to copy.
- **Deleting the session** removes its queue and its composer directory.
- **A session's queue holds at most 100 messages**; one more is refused with `limit_exceeded` (`limit`
  `queued_messages`, `max` 100).

## The queue in the composer

While the composer is on and the session has queued messages, they are listed above its editor, in queue order, as a
list named **Queued messages**. Each message's row shows:

- the start of its text on one line, in a monospaced face for a slash command;
- its mode, **When idle** or **Manual**, or **Sending** with a wait indicator while it is being handed over;
- **Command** for a slash command, and how many files it carries;
- **Not delivered** or **Refused**, with the reason, for a message that came back (see "Messages that come back"
  above);
- **Editing** while it is being edited in the composer, the row tinted.

Each row ends with **Send now** (unavailable while the message is being sent or edited), **Withdraw**, and a **Message
actions** menu holding **Edit** (see "Editing a queued message" in `docs/product/message-composer.md`), the two modes
**When idle** and **Manual** with the current one marked, and **Move earlier** and **Move later**. With more than one
message, a row can also be dragged by its handle, with a mouse or a pen, to another place; the menu's moves are the way
for every input. The order shown is the daemon's: a drop shows the new order until the daemon's queue changes, and
falls back where the move is refused. When a row leaves with keyboard focus in it, focus goes to the Withdraw of the
row now in its place, else of the row before it (never to Send now, where a repeated Enter would send it), else to the
composer's text area. A failure raises an error alert saying the message could not be sent, could not be withdrawn, or
the queued message could not be changed.

## The queue line with the composer off

In a window with the composer off, while the selected session — not archived — has queued messages, a compact line over
the top of its terminal says how many are queued, so messages queued from another client never go out unseen. It
comes after the banner of standing notices and the upload notice among the bars over the terminal (see `docs/product/standing-session-notices.md` and
"The upload notice" in `docs/product/dropping-files-onto-a-terminal.md`). Its **View** button opens a popover listing
the messages as the composer does, each with only **Send now** and **Withdraw**. Screen readers are told once, as the
line appears under the open session, how many messages are queued.

**Keyboard**: the line is a stop of its own in the `F6` cycle, just before the terminal, while it shows (see "Moving
focus between regions with F6" in `docs/product/moving-focus-between-regions.md`); `F6` lands on View. When the line
goes while one of its controls holds keyboard focus, focus returns to the terminal.

## Requests and events

A queued message as the daemon sends it:

```
QueuedMessage { id, text, images: string[], files: string[], command, mode: "when_idle"|"manual",
                state: "waiting"|"sending",
                failure?: { kind: "not_delivered"|"refused", code, params, message },
                created_at }
```

`images` are attached as the message's images; `files` are written after the text, on a line of their own, each bare
or in single quotes as a POSIX shell quotes it, joined by spaces; `command` marks `text` as a slash command, sent as
`send_command` sends one. A command's text is stored as it will be sent, with its control characters removed and its
surrounding whitespace trimmed, by `create_queued_message` and by an `update_queued_message` that gives `text` or
`command`. `failure` is the mark of a message that came back, with the code and parameters of the
notice or refusal.

| Request | Fields | What it does |
|---|---|---|
| `create_queued_message` | `session`, `text`, `images?`, `files?`, `command?`, `mode?` | Puts a message at the end of the queue of a session that is not archived, running or not. `mode` is `now`, `when_idle` or `manual`, defaulting to the session's default; `now` sends it at once, as `send_queued_message` does. `images` are at most ten paths `save_pasted_image` answered for this session. Answered with `queued_message_created` (`id`, `message`, the new message's id) |
| `update_queued_message` | `session`, `message`, `text?`, `images?`, `files?`, `command?` | Changes a waiting message: a field left out stays, a list given replaces the message's whole list. Answered with `ack` |
| `set_queued_message_mode` | `session`, `message`, `mode` | Sets the mode a message waits in, `when_idle` or `manual`. Answered with `ack` |
| `move_queued_message` | `session`, `message`, `position` | Moves a message to `position`, counted from 0 among the other messages; past the end puts it last. Answered with `ack` |
| `send_queued_message` | `session`, `message` | Sends a waiting message now (see "Sending a queued message now" above). Answered with `ack` once delivery has written or queued it, or the resume has started |
| `withdraw_queued_message` | `session`, `message` | Takes a message off the queue, deleting the files it owns (see "Files of queued messages" above). Answered with `ack` |
| `set_session_send_mode` | `session`, `mode` | Sets the session's default send mode, `now`, `when_idle` or `manual`. Answered with `ack`; every client is sent the session again (`session_upserted`) |

Every change to a queue sends every client `session_message_queue` (`session`, `messages`), the session's whole queue
in order, empty once nothing is queued, the session archived or deleted included. `snapshot` carries `message_queues`,
the queue of every session that has queued messages. The session's default send mode is `Session.send_mode`.

Refusals:

- `create_queued_message` and `update_queued_message`: `unknown_session`; `session_archived`; `field_required` (`field`
  `text`) for a message with neither text nor an image, or a command with images or files (for
  `update_queued_message`, judged on the message as the edit leaves it, fields left out included); `unreadable_request`
  (`field` `images` or `files`) for an image that is not one of this session's, more than ten images, a path named more
  than once, or a file another request has moved already, and (`field` `command`) for a command that is not one line
  beginning with `/` and its name; `provider_unsupported` for images to an agent that takes none, or a command to one
  that takes no slash commands; and, for `create_queued_message`, `limit_exceeded` past 100 messages. A refused request
  moves no file.
- `update_queued_message`, `set_queued_message_mode`, `send_queued_message` and `withdraw_queued_message`:
  `queued_message_not_found` for a message the queue no longer holds (sent, withdrawn or dropped since), and
  `queued_message_sending` for one being handed over (for `withdraw_queued_message`, one being handed over or written).
  `move_queued_message` is refused with `queued_message_not_found`, and a negative `position` with
  `unreadable_request`.
- `send_queued_message` answers a refusal of delivery with that refusal's error, and sets it as the message's mark.
- `set_session_send_mode`: `unknown_session`.
