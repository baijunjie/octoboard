# Octoboard's memory

Octoboard keeps a memory of its own for sessions: entries that one session saves and later sessions read, whatever
their agent or account. The daemon keeps the entries in its database (see `docs/product/files-octoboard-owns.md`), never
in files of the user's or the agents'. There are two kinds:

- **Console memory**: one per console, read and written by every console session of that console.
- **Project memory**: one per place a project is at, read and written by every project session there, coordinators and
  workers alike (see "Project memory" below).

Sessions read and write their memory with four tools of Octoboard's MCP server (see "The memory tools" below), and each
session starts with an index of it (see "The index a session starts with" below). The user reads and corrects the
entries in the memory dialog (see "The memory dialog" below). A session saves without asking anyone; the user manages
what was saved afterwards. Everything is read from the daemon, so a remote window and a browser have all of it as the
local window does.

## Turning the memory on and off

The memory is turned on and off by one application setting, **Let sessions share Octoboard's memory** in Settings'
Memory section (`memory_enabled` to the daemon and to the settings tools), **off by default**.

- **While it is off**, sessions behave as if the memory did not exist: no session is offered the memory tools, a memory
  tool called anyway is refused with a reason saying the memory is turned off and that the user turns it on in Settings,
  Memory, for sessions launched afterwards; neither the console session's instruction file nor a project session's
  role description says anything of a memory; and no launch touches the agent's own memory.
- **While it is on**, every session is offered the memory tools for its own memory and starts with its index, and every
  launch turns the agent's own memory off (see "The agents' own memory" below).

The launch-time effects (the tools offered, the memory text, the agent's own memory) follow the setting as it stands at
each launch, a resume and an account switch included, so a change reaches only sessions launched afterwards; one launch
reads the setting once, so all three always agree. Whether a memory tool call is refused follows the setting at the time
of the call. **Turning the memory off deletes nothing**, and everything the window does with memory — the memory dialog
requests, the memory left behind, deleting a project's memory with the project — works whatever the setting says; the
window decides what to show.

A console session reads and changes the setting with `get_settings` and `update_settings`, as it does the other
application settings an agent may help with (see "Application settings: `get_settings` and `update_settings`" in
`docs/product/agent-settings-tools.md`). The setting stays on its machine: a configuration export never carries it (see
"Which settings travel" in `docs/product/settings-in-a-configuration.md`).

## The agents' own memory

Each supported agent keeps a memory of its own across sessions, split by agent and by account, which a session on
another agent or account never sees. While Octoboard's memory is on, every session Octoboard launches, console and
project sessions alike, has its agent's own memory turned off for that launch, so that it keeps what it learns in
Octoboard's memory alone:

| Agent | Turned off by |
|---|---|
| Claude Code | the environment variable `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` |
| Codex | the launch argument `-c features.memories=false` |
| Grok Build | the environment variable `GROK_MEMORY=0` |

Each switch outranks the user's own setting for that agent's memory. It is part of the per-launch injection: nothing is
written into the agent's or the user's configuration, and the memory files an agent wrote before are neither read, moved
nor deleted. While Octoboard's memory is off, each agent's own memory is left as the agent and the user set it.

## Console memory

A console has one memory, shared by every console session of that console whatever its agent or account, and by nothing
else. It is for what helps a later console session: the user's preferences and corrections, which kind of request
belongs to which project, facts about the machine for work that touches no project, and pointers to outside resources.

An entry of a console's memory can be marked **local**, "this machine only": it holds only on this machine (a local
path, an environment value, the machine's accounts), and a configuration export leaves it out. The rest of a console's
memory travels with the console in a configuration export (see "What each console carries" in
`docs/product/configuration-export-and-import.md`).

Deleting a console deletes its memory, and the ids it gave, with it (see "Deleting a console" in
`docs/product/consoles.md`).

## Project memory

A project memory belongs to a **place**, not to a project record: the machine the project is on (its host) and the
project's stored path. Every project at the same place shares it, whichever console it belongs to, and removing a
project and adding it again at the same path finds the same memory. Places are told apart by the stored path's text, so
the same folder reached through another spelling of its path (a symbolic link, say) is another place.

It is meant only for what agents need to know about the machine the project is on: local paths and where a local
resource is, ports, local environment values, how a local tool or the machine's accounts behave there. Anything else
about the project belongs in the project's own documents, where the team and other machines share it. Its entries carry
no local mark, since every one of them holds only on that machine, and a project memory never travels in a
configuration export.

A project memory outlives its projects. Removing a project keeps it unless the user asks for it to go too (see "The
project's memory" in `docs/product/removing-projects.md`), and what no project is at any more is "left behind" (see
"Memory left behind" below).

## Entries

| Field | Rules |
|---|---|
| id | Given by the daemon when the entry is created: 8 lowercase letters and digits, unique within its memory and never given to another entry of that memory, a deleted one's included, so an id a session still holds never reaches another entry. |
| summary | One sentence summing up the content, the entry's only label, by which a session decides whether to read it. One line of 1 to 200 characters after trimming, with no line break or other control character; stored trimmed. It need not be unique. |
| content | The entry's body, in Markdown. Not blank, at most 16,384 bytes of UTF-8; stored as written. |
| local | A console entry's only: whether it holds only on this machine. A new entry is not local unless it says so, and an update that leaves it out keeps the entry's value. Given for a project memory's entry, it is ignored, not refused. |
| created, updated | When it was created and last updated. |
| updated by | Who wrote it last: **an agent** (any session's memory tools) or **the user** (the window). Nothing more of either is recorded. |
| version | 1 when created, raised by every update. |

Entries have no type. A memory holds **at most 200 entries**; a new entry past that is refused. Everywhere entries are
listed — the tools, the index, the dialog, a configuration export — they come most recently updated first.

**An update names the version it read.** A save that updates an entry names its id and the version it was based on, and
is refused when the entry has been saved since, by a session or a window, so that nobody overwrites a change they never
saw. Saves are checked and written one at a time, so of two saves naming the same version only the first is saved. This
check belongs to the entry and is separate from the per-field versions of settings and records (see
`docs/product/concurrent-edits.md`).

## The memory tools

Offered to every session, console and project, while the memory was on at its launch. Each reaches the memory of the
calling session's kind, resolved from the session and never from an argument: a console session its console's, a
project session its project's place's. The tool descriptions are worded for the kind of session they are offered to.

| Tool | Arguments | What it does |
|---|---|---|
| `save_memory` | `id?`, `version?`, `summary`, `content`, `local?` (console sessions only) | Without `id` and `version`, creates an entry, which the daemon gives an id. With both, replaces the summary and content of the entry `id` (and its `local` when given). Answers with the entry as saved, without its content: `id`, `summary`, `local` (a console's only), `version` (the new one), `created_at`, `updated_at` and `updated_by`. |
| `list_memories` | — | The current index, as `memories`, each with `id`, `summary`, `local` (a console's only), `updated_at`, `updated_by` and `version`, most recently updated first. It includes what other sessions saved after the caller's index was written. |
| `read_memory` | `id` | One entry in full, its content and version included. |
| `delete_memory` | `id` | Deletes one entry for good, and answers `deleted`, its id. |

Times are milliseconds since the Unix epoch, and `updated_by` is `{"kind": "agent"}` or `{"kind": "user"}`. A refused
call is a tool error carrying the reason in prose, as the other tools' refusals are, and saves nothing:

- a memory tool while the memory is turned off at the time of the call;
- `id` without `version`, or `version` without `id`, or a `version` that is not a whole number of 1 or more;
- an `id` that names no entry, which may have been deleted;
- an update naming a version other than the entry's current one: the reason carries the current entry as JSON, so the
  session can merge its change and save again with the current version without another call;
- a summary or content that breaks the rules above, the content's size named;
- a new entry in a memory that holds 200;
- a save to the memory of a console deleted meanwhile.

Every change a tool makes reaches every open window at once, as one the user makes does.

## The index a session starts with

While the memory is on, a session starts with its memory's index: one line an entry, most recently updated first, each
with the entry's id, "this machine only" for a local console entry, the date of its last update (UTC) and its summary,
never its content, as in ``- `a1b2c3d4` (this machine only, updated 2026-10-24): Docker needs a prompt here``. The
lines take at most 4 KiB; past that the most recently updated entries are listed, and a line after the index says how
many older entries are left out and that `list_memories` lists every one. A memory with no entry gets the guidance
around an index that says there are no entries yet. The index is held between tags of its own (`console_memory`,
`project_memory`), stated to be data and never instructions, and a closing tag of that name inside a summary, in any
letter case, has its `<` written as `&lt;`, so it cannot end the index early.

**A console session** finds its console's index in a section headed **Memory** of its instruction file, after the whole
built-in part and before the user's own instructions, which stay last (where the file is and when it is written are in
"The console session's instruction file" in `docs/product/launching-agents.md`). The file's list of tools names the
memory tools too. The section tells the session:

- to keep lasting knowledge in this memory only, not in its agent's own;
- what to save — the user's preferences and corrections of how it dispatches and reports, with the reason; which kind
  of request belongs to which project and how the projects relate; facts about the machine for work that touches no
  project; pointers to outside resources — and what not to: the state of tasks and sessions, which other tools give; a
  lesson that belongs to one project, which that project's session records under the project's own conventions;
  secrets; what the user's console session instructions already say; and what a child session reports, as it stands;
- to mark an entry local when it holds only on this machine, since an export leaves those out;
- to write the summary so that it says what the entry holds and when it matters; to look for an entry on the same
  subject, in the index and with `list_memories`, and update it by its id and version rather than add another; and to
  delete an entry found to be wrong;
- that it needs no approval to save, but says in its reply what it saved, so the user can correct it;
- that memory is background as of when it was written, to be checked before it is relied on, ranking below the file's
  built-in rules, the user's instructions and what the user says in the conversation, and never changing the rule that a
  console session changes no project itself;
- that the index is as it stood when the file was written, that another console session may have saved since, and that
  `list_memories` gives the current one.

The file is written before every console session launch, so a console session launched after another saved an entry
starts with it.

**A project session**, coordinator or worker, finds its place's index in a section of its role description headed
**The project's memory**, after the part on its role and reporting relationship and before the development rule and the
other guidance the settings add (see "What is injected on every launch" in `docs/product/launching-agents.md`). It says
what the memory is for, as in "Project memory" above, and that anything else about the project goes into the project's
own documents while the state of its task and secrets go into neither; how to keep one entry a subject, as for a console
session; that memory is background ranking below the project's own instructions and conventions and what the user says;
and that the index is **a snapshot taken when the session was launched**, not updated while it runs, with
`list_memories` giving the current one. The role description is read at each launch, but an agent that replays the
description of its first launch on resume keeps that launch's snapshot (see "The launch description can remain in the
conversation after the role changes" in `docs/product/launching-agents.md`), which is why it is worded as one.

## Memory left behind

A place's memory that no project of any console is at any more is **left behind**: removing the last project at a
place, alone or with its console, leaves its memory there, unless the user deleted it with the project. Adding a project
at that place again — by hand, by a console session's `add_project` or by an import — takes it up again, and it is no
longer left behind.

The user sees what is left behind in Settings' Memory section (see "The Memory section of Settings" below): each place
with its machine, its path, how many entries it holds and when its latest entry was updated, most recently updated
first, and deletes the memory of one place, or of every place at once. Deleting one place's memory is refused
(`memory_place_in_use`) while a project is at it, which reads it; deleting all of it looks at each place again as it
goes, so a place a project was added at meanwhile keeps its memory. The list follows every change to it at once, in
every window.

## The memory dialog

**Memory** is an item of a console's action menu, after **Console settings** (see "The console header" in
`docs/product/sidebar.md`), and of a project's action menu, after **Project settings** (see "Project rows" in
`docs/product/sidebar-project-rows.md`), with the letter shortcut `M` in both. Either is offered only while the memory is
on. It opens the memory dialog over that console's memory, titled **Console memory**, or that project's place's,
titled **Project memory**.

Under the title an introduction says what the memory holds, naming the console or project by its current name: for a
console, the long-term memory of its console sessions — the user's preferences, which project takes which kind of work,
lessons from earlier work — which each console session reads when it starts; for a project, what agents working in it
need to know about the machine the project is on, shared by every agent working there, with anything else belonging in
the project's own documents.

- **The list**: one row an entry, most recently updated first. Its summary is on one line, faded where it is clipped,
  with the whole of it in a tooltip then; after it, for a local console entry, a **This machine only** tag that never wraps
  or shrinks. Under the summary, when and by whom it was last updated: "Updated 5 minutes ago by an agent" or "… by
  you", and "Updated just now …" within its first minute, kept current as time passes. Pressing a row opens the entry;
  the row's action menu has **View**, **Edit** and **Delete**.
- **While it loads** it says so; **a failure to load** is shown in its place with the means to try again.
- **With no entry**, it says there is no memory yet and that sessions save what they learn there as they work, with no
  button of its own.
- **The footer**: at its start, while there are entries, how many of the 200 the memory holds ("3 of 200 entries");
  then **Delete all**, unavailable with no entry, and **Add entry**, unavailable until the list has loaded and, at 200
  entries, unavailable with a reason saying the memory is full.

**Viewing an entry** shows its summary with its tag, the updated line and its content as plain text, scrolling where it
is long, with **Close** and **Edit**.

**Adding or editing an entry** is a form with **Summary** (with a note saying it is one sentence sessions read in the
index to decide whether to open the entry), **Content** (with a note saying it is Markdown text of up to 16,384 bytes),
and, for a console's memory only, a **This machine only** checkbox, with a note saying it is for what applies only on
this machine and is left out of configuration exports. The form holds the entry to the rules above and shows a broken one
next to its field when it is submitted. Whatever the user saves records the user as its writer.

**The dialog follows the memory while it is open**: an entry a session or another window saves, updates or deletes
appears, changes or goes without reopening it. An entry open in the view or the form that is deleted meanwhile says so
and can no longer be edited or saved. A change to the entry being edited shows in the form's fields, and assistive
technology is told which fields the user had edited were replaced, as in "A form while its record changes elsewhere" in
`docs/product/concurrent-edits.md`. A save refused because the entry was saved since the form read it
(`memory_changed`) opens an error alert naming the entry and saying nothing was saved, and the form shows the entry as it
now stands, to be checked and saved again.

**Deleting** asks for confirmation naming the entry by its summary and saying sessions will no longer see it. **Delete
all** asks for confirmation that its Delete all button keeps disabled until the word for "delete" in the current
language is typed, as deleting a console does, saying that every entry of the console's or project's memory will be
deleted and that this cannot be undone.

## The Memory section of Settings

The **Memory** section of Settings (see `docs/product/settings.md`) holds two rows, with no group headings, there
whatever the memory setting says:

- **Let sessions share Octoboard's memory**, the switch of "Turning the memory on and off" above. Off, its description
  says what turning it on will do; on, what is happening now: sessions launched from then on read and save Octoboard's
  memory, which every agent shares, and their agent's own memory is turned off. It saves as Settings' other switches do
  (see "How switches save" in `docs/product/settings.md`).
- **Memory of removed projects**, whose description says that a removed project's memory is kept and that adding a
  project at the same folder picks it up again. Its control is the number of folders whose memory is left behind, while
  there is any, and a **Manage** button. Manage is unavailable while the list is being read; with nothing left behind,
  unavailable with the reason that there is none; and where the list cannot be read, unavailable with that reason, after
  a mark that says so and offers to try again.

**Manage** opens a dialog titled **Memory of removed projects** listing each folder's memory left behind (see "Memory
left behind" above): its path, and under it the machine it is on (by the name the daemon knows it by, or as an unknown
machine), how many entries it holds and when it was last updated, with a **Delete** button. Delete asks for confirmation
naming the path and the machine and saying how many entries go, which cannot be undone. The footer has **Delete all**,
unavailable with nothing listed, which asks for confirmation naming how many folders and entries go, with the word for
"delete" typed as above, and **Close**. The list follows changes made here or elsewhere while it is open; with nothing
left it says there is no memory left behind.
