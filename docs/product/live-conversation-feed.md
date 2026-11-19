# Live conversation feed

A client connected to the daemon's control socket can follow one session's conversation as it happens: the daemon reads
the agent's own record of the session (the record the session history reads, see `docs/product/session-history.md`)
and sends the client the conversation as structured entries, first the latest of them, then what the agent adds as it
works. The desktop window does not use the feed; it shows a live session's terminal, and an archived session's
history as `docs/product/session-history.md` says.

The feed is read from what the agent writes to its own files, so what it shows of a turn in progress, and how soon,
depends on the agent (see "Per agent" below), and is no promise about what a later version of an agent will write.

## Following a session

- **A client starts following a session with `subscribe_session_conversation`** and is answered with the latest page of
  the conversation (see "Pages" below), the entries the agent is still producing (see "Pending entries" below) and the
  epoch they belong to. From then on the changes come to that connection alone, until it sends
  `unsubscribe_session_conversation` or the connection ends. Asking again for a session already followed answers with
  a fresh latest page.
- **Any session can be followed, in any status**: an archived session's conversation simply stops growing. A session
  whose record does not exist yet is an empty conversation, looked for until it appears. An id with no session is
  refused as an unknown session.
- **A connection follows at most 16 sessions at once.** A subscription to a 17th session is refused, naming the limit,
  and nothing is followed; unsubscribing from one, or the feed of one ending, frees its place.
- **Nothing is stored.** The daemon reads a conversation only while some connection follows it, shares one reading
  between the connections that follow the same session, and drops it when the last of them leaves or the session is
  deleted. The feed is no part of what every connection is sent about sessions, and nothing is written to the agent's
  record.

## What the conversation holds

The conversation is a list of entries in order. Each entry has its position in the conversation (counted from 0 under
the current epoch), the turn it belongs to, the time the record gave it where it gave one, and a kind:

- **User**: what the user typed. A slash command the user invoked reads as typed, `/name args`.
- **Octoboard**: a message Octoboard wrote into the session — a brief, a parent session's instruction, a report, shared
  information, a notice. It is told from what the user typed by how it begins, by the headers in
  `docs/product/message-headers.md`, as the session history tells it. Unlike the session history, the feed shows these
  messages, as their own kind.
- **Assistant**: text the agent wrote, a progress note on the way as much as a turn's answer. Unlike the session
  history, every such text of a turn is an entry, not only the last.
- **Tool call**: a tool the agent called, as one entry that the call's result later updates (see "Tool calls" below).
- **Todos**: the agent's todo list as it stands after this point, whole, each item with its text and the agent's own
  status word (pending, in progress, completed or cancelled).
- **Compaction**: the agent compacted its context here, with the summary it wrote where its record keeps one.

A user or Octoboard entry opens a turn, turns counting from 1; the entries after it share its number, and entries before
the first of them belong to turn 0. The agent's reasoning is not an entry, and a subagent's own conversation is not
followed: only the call that started the subagent is.

A client ignores an entry of a kind it does not know, since a later daemon of the same protocol generation may add
kinds; a tool call's category it does not know reads as other, and a status it does not know as running (a later
daemon may add a state in which a call has not ended yet).

### Tool calls

A tool call entry carries:

- the agent's own id of the call and the tool's name as the agent names it;
- a **category**, the same whichever agent made the call: command, file edit, file read, search, web, MCP, subagent or
  other;
- where one can be told, a one-line **title** saying what the call does: the command it runs, the file it reads or
  edits, what it searches for;
- the agent's **input** to the call, with each string longer than 16 KiB cut, and a mark when one was;
- the **files the call changed**, each spelled as the agent spelled it — not checked, and possibly relative to the
  session's working directory — and none for a call that changed no file;
- a **status**: running, completed, failed, or declined for a call that was not allowed to run (the user or a permission
  rule declined it);
- once the call has ended, its **output**, cut to its first and last 8 KiB when longer, with a mark when it was, and a
  command's **exit code** where the record gives one.

When the result comes, the client is sent the whole entry again, its status and output filled in.

## Pages

- **A page is the latest entries up to its size**: 50 when the client gives none, at most 200, and also no more than
  1 MiB of entries, though always at least one entry. A page says whether the conversation has entries before it.
- **A client scrolling back reads the page before the earliest entry it holds** with `read_session_conversation`,
  naming that entry's position and the epoch it holds. For an epoch that is no longer the current one the page is
  empty and says the current epoch. Reading is refused as not followed for a session the connection does not follow;
  nothing is read then.

## Live changes

While a client follows a session, each change is sent as one event naming the session and the epoch:

- the **entries appended** after the last one;
- the **entries updated** that the client already holds, each whole, named by its position — a tool call reaching its
  result, for one;
- the **pending entries**, all of them, whenever they changed.

An event carrying more than 1 MiB of entries is split into several. An event can repeat what a reply or an earlier event
already gave, so an appended entry at a position the client already holds replaces that entry, and no entry is ever
held twice. A client drops an event or a reply of an epoch other than the one it holds, a replacement excepted.

### Pending entries

Pending entries are what the agent is still producing that its record does not hold yet, where its files show any:
text and tool calls in the shape of the other entries, without a position. They are shown after the conversation's
entries and are replaced whole each time they change; once the record holds an entry, it takes the pending one's place
among the appended entries.

## When the conversation is replaced

When what a client holds no longer stands, the daemon sends a **replacement**: the latest page under a new epoch, with
the pending entries. The client discards everything it held of the session, entries and pending ones, and takes the
replacement as it is. This happens when:

- the agent went back to an earlier point of the conversation (a Claude Code rewind, a Grok Build rewind) or rewrote its
  record (a Grok Build compaction);
- the agent started another file for the conversation (a Codex thread that had a turn reverted continues in a new
  file);
- the record was replaced;
- a connection fell too far behind the changes: it is sent one replacement instead of what it missed.

A conversation read anew that holds the same entries as before keeps its epoch, and nothing but a change of the pending
entries is sent. The daemon looks for the files a conversation is read from again at once when the session's agent
session id, account directory or recorded path changes, and otherwise every few seconds while it finds none and every
15 seconds while it reads them cleanly, so a new file the agent starts on its own is taken up within 15 seconds.

## When the feed ends

When the daemon cannot go on following a conversation — the session was deleted, or the daemon failed while reading
it — it tells every connection following it that the feed ended, and their subscriptions are gone: nothing more is sent
for the session, and the places the subscriptions held among the 16 are free. A client that still wants the
conversation subscribes again; a deleted session is then refused as unknown. Reading earlier entries after the end is
refused as not followed, and so is a read the daemon takes once it has stopped following the conversation, the notice
of the end following that refusal.

A record that cannot be read at one look ends nothing: it is read again at the next look, or, when a first read failed
part-way through, once the file has changed. A connection's own unsubscribing is not told as an end.

## Per agent

- **Claude Code**: a tool call appears as soon as the agent makes it, before it runs, as running, and its result
  updates it; a call the user or a permission rule declined, in any of Claude Code's ways, reads as declined. Text
  arrives as whole blocks only, since the transcript writes a text block when it is complete: there is no text in
  progress and no pending entries.
- **Codex**: entries are read from the thread's rollout only, so there is no text in progress and no pending entries;
  Codex's live notifications from its app server are not used. A call Codex records only once it is over appears with
  its result already. A call Codex records when the model makes it, with its output recorded separately, appears as
  running until that output; one that never gets an output is ended with its turn: failed when the turn was cut off or
  ended in an error, completed otherwise.
- **Grok Build**: Grok writes a model call to its record whole when the call ends, with its text and the tool calls it
  made, before those tools run, so text arrives whole. A tool call is running until its result is recorded; whether it
  failed is taken from the update log Grok writes beside its record, since the result itself carries no failure mark.
  The update log is written a few tens of milliseconds after the record, so pending entries show only in that short
  interval, if at all. A compaction leaves Grok's record holding only what followed it, so a compacted session's
  conversation starts at the compaction.
