# Slash commands and image messages

Besides a message of text, a client of the daemon can send a running session one of its agent's own slash commands to
run (`send_command`), and a message with images attached (`send_message` with `images`). Both are the user's own
writes, held to the rules of a client's `send_message` (see "Messages held until a session can take them" in
`docs/product/message-delivery.md`): refused while the session waits for the user (`session_waiting_for_user`) or while
its trust confirmation is, or may still be, up (`session_trust_pending`); written past a hold after a usage limit; and
otherwise queued behind what is queued for the session and written under the same holds and the same draft
preservation as any message (see `docs/product/draft-preservation.md`). Octoboard's own window sends neither today; it
shows the failure notices below.

## Which agents take them

Each provider declares, in its descriptor's capabilities, whether it takes images attached to a message
(`message_images`) and whether it runs a slash command sent as one (`slash_commands`). All eight supported agents declare both, including OpenCode. A provider that does not is refused the request with `provider_unsupported` (operation
`message_images` or `slash_commands`), and nothing is written. So is a slash command with no arguments for a provider
whose input box Octoboard has no measured way to clear, since a wrong completion could not be taken back (see below):
Ctrl-U pressed a line at a time, or a key the provider declares that empties the whole box at once at its idle prompt
(GitHub Copilot CLI's Ctrl-C, whose Ctrl-U empties only one line of a draft). Such a key is pressed once, only while
the session reads at its idle prompt with text in the box, and never mid-turn, where it may interrupt the turn; a clear
needed mid-turn falls back to Ctrl-U where the provider has it, and otherwise leaves the box as it stands. Saving the image a message would name is refused the same way for a session of a
provider without `message_images` (see "The request" in `docs/product/pasting-images.md`), so such a session has no
image to send.

## Sending a slash command

`send_command` takes the session and `command`: one line beginning with `/` directly followed by the command's name,
with any arguments after it. Control characters and the characters Claude Code treats as invisible are removed and the
line is trimmed first; anything else is refused with `unreadable_request` (`field`: `command`). The request is answered
with `ack` once the command is written.

- **Written as a command.** It gets none of the reshaping that keeps a message's leading `/` from running as a command
  (see "Messages held until a session can take them" in `docs/product/message-delivery.md`), and it always goes into
  the agent's terminal, also on a Codex session whose messages go into Codex's own queue, which would hand it to the
  model as text; there it takes the terminal's draft checks.
- **Never beside other text.** A command goes only into an input box found empty, or emptied by the draft preservation's
  stash or clearing. Where the box holds text that cannot be cleared, a message would go in with it once its 30-second
  wait is over (see "When the composer cannot be cleared" in `docs/product/draft-preservation.md`); a command is refused
  instead, as not written (below), since the text would turn it into another.
- **Checked against the agent's completion.** The command is pasted on its own and the screen looked at before its
  Enter: each agent's slash-command completion runs the command it highlights on an Enter, not what was written. Claude
  Code ran `/probecmd` for a pasted `/probe`, Codex runs `/status` for a typed `/sta`, and Grok Build only completed a
  pasted prefix and sent nothing. Where the completion offers another command than the one written, the command is
  cleared from the input box and not sent, and the request is refused with `command_not_recognised` (`agent`,
  `command`, `offered`: the command offered, with its slash and without arguments). Where the agent's list says no
  command matches what was written (OpenCode's `No matching items`), the command is cleared and refused with a code of
  its own, `command_not_found` (`agent`, `command`): an Enter would have sent it to the model as a prompt. Where the clearing keys were refused or did
  not show, the command may be left in the box. A completion shows only for a lone `/name`, so a command with
  arguments has nothing to be checked against.
- **A space after a lone command, where the agent needs one.** A provider whose first Enter on a lone `/name` takes its
  completion into the box rather than running it (OpenCode) has a space typed after the command, as a key of its own,
  once the completion has been looked at and only while the provider reads its list up; the Enter follows only once
  the provider reads that list closed, which shows the space landed. No list read up before the space leaves the
  command in the box unsent, with no space typed; a list still up after it, or one the provider cannot tell of, leaves
  the command and its space there; either is a command left in the box.
- **Sent once the box empties.** An agent's own command reports no prompt, so its Enter is confirmed by the input box
  emptying, not by a hook. A command left in the box is a message left unsent, with its notice and the hold on later
  messages (see "A message written but not sent" in `docs/product/message-delivery.md`); its Enter is not pressed again.

## Sending images with a message

`send_message`'s `images` lists at most ten images, each a path `save_pasted_image` answered for this session: a file
saved in the session's own pasted images directory that still exists (see "Where the image is kept" in
`docs/product/pasting-images.md`). More than ten, or any other path, is refused with `unreadable_request` (`field`:
`images`). With images, `text` may be empty; it is reshaped as any message's.

- **Through Codex's own queue.** On a Codex session whose messages go into Codex's own queue (see "Messages through
  Codex's own queue" in `docs/product/message-delivery.md`), the images go into the queue with the text, as local
  images Codex reads when it takes the message, and that queue's rules apply.
- **Through the terminal**, for every other session: each image's path is pasted on its own, and the next paste waits
  until the agent's input box shows one more image placeholder (`[Image #n]`); then the text, if there is any, and the
  Enter, as for any message. A placeholder that does not show in time leaves the message in the input box, a message
  left unsent as above; nothing more of it is pasted.
- **Inside one draft-preservation step.** The first paste goes in through the same check of an empty input box, stash
  or clearing as any message, and the rest of the message within that same step, so a draft the agent stashed around
  the message comes back only after the Enter. Unlike a command, a message with images goes in beside text that could
  not be cleared once its wait is over, as any message does.
- Grok Build drops an image it finds too small when the message is sent, with a notice on its own screen; the size it
  draws the line at is not known.

## Queued commands and image messages

Neither is ever carried as text, since a command written as text would reach the model and an image is a file that
goes away with the session's process:

- **Images gone.** Pasted images are removed when the session's process ends, so a message with images still queued
  then — carried into the process an account switch or a change of model relaunches, say — is refused when its turn
  comes, as not written. Later messages go on.
- **A planned stop.** A command or a message with images still queued when the terminals are handed over is not saved
  for the next daemon: it is dropped, and only the daemon's log records it (see "What is queued crosses a planned
  restart" in `docs/product/message-delivery.md`).
- **A handoff.** One queued for a session being handed off is not forwarded to the receiving session: it is dropped,
  and counted in the `handoff_messages_dropped` notice about the receiving session (see "Messages for the original
  during a handoff" in `docs/product/handing-off-sessions.md`).

## The failure notices

A command or a message with images that fails this way fails alone: it is not sent by any route or written again, and
the messages queued behind it go on. Its sender, where it waits for the answer, is refused with the error of the same
code; every client is sent the notice about the session in any case.

- **`message_not_written`** (`agent`, `detail`): the message could not be written as it was sent, through no fault of
  the agent's — its images are gone, or a command could only have gone beside text the input box holds and that could
  not be cleared. The window shows it as an error alert titled "Message not sent", saying a message queued for the
  session could not be written, so it was not sent, and that later messages are still sent, with the reason
  (`detail`, English) under its details.
- **`command_not_recognised`** (`agent`, `command`, `offered`): the agent would have run `offered` instead of the
  command written. The window shows it as an error alert titled "Command not sent", saying which command would have
  run, and to clear the command from the input box if it is still there.
- **`command_not_found`** (`agent`, `command`): the agent has no such command. The window shows it as an error alert
  titled "Command not sent", saying the agent has no command of that name, and to clear the command from the input
  box if it is still there. A window older than this code shows the daemon's English message.
