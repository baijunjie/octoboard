# Website product preview

## Product preview

The preview shows the actual application's interface populated with fictional data and identifies itself as a sample.
Three buttons select the project overview, focus mode, or code review; the project overview is the initial scene. All
three scenes show the same ongoing development and integration snapshot for the restaurant meal deal promotion across
the eight independent example projects named in `docs/product/website.md`, "Homepage and navigation". They show the
coordinating and project sessions, a project in focus mode with its files and shell terminal, and a selected
order-validation diff. Choosing a scene resets the demonstration to that view of the shared snapshot. The coordinator's
terminal gives the same progress summary in every scene.

The focus-mode scene opens Restaurant order API's project focus mode, including its three sessions bound to the
coordinator. Its working Codex session is selected. On desktop, the Files and Terminal panels open together beside
the agent terminal, with Files above Terminal. The Terminal panel contains a sample `zsh` shell whose `pwd` and
`git branch --show-current` output match the project's directory and branch.

Seven sessions, including the coordinator, are working. The sessions for Promotion console and Checkout register await
instructions after completing their current checks, with integration feedback still pending. Restaurant order API's
order-validation review session awaits instructions while a test watcher (`pnpm test --watch`) runs in the background,
so it shows as working in background. Its order-checking session waits for the user to confirm the discount rule. These
statuses retain their application meanings in `docs/product/session-statuses.md`.

Each session's agent terminal imitates its agent's own terminal interface: Claude Code, Codex and Grok Build each
have a distinct header, prompt, tool-call rows, input area and hints. These looks are approximations and name no
model or version. A terminal shows the session's task prompt, then what its agent has done so far (file reads, edits
with their added and removed line counts and, for some, the changed lines, command runs and the agent's remarks),
then a state that follows the session's status:

- A working session shows a status line with a spinner and an elapsed timer, and appends further steps from its own
  short list 4 to 7 seconds apart; once the list is used up, only the spinner and timer keep running. Each session's
  timer starts from a different sample elapsed time. Progress counts from when the preview loaded, so reopening a
  session shows the steps already reached instead of replaying them, and a tab returning from the background catches
  up at once.
- A session awaiting instructions shows its finished summary and an empty input area; the order-validation review
  session's terminal also names its running background task (`pnpm test --watch`).
- The order-checking session waiting for the user shows its agent's question on whether meal deals can be combined
  with member discounts, with numbered choices and keeping them separate selected.

Visitors who prefer reduced motion see static terminals: no spinner motion, no appended steps and a fixed timer. The
file reads, edits, test results and the coordinator's diff statistics shown in the terminals are derived from the
sample repositories, so they agree with the files and diffs the visitor can open. The terminals carry no sample-data
line of their own; the caption or label described below identifies the preview as a sample.

Each of the eight projects has its own sample repository, with a distinct root, feature branch, business files and
Git changes. Selecting another project shows that project's source and diffs. The examples cover promotion
configuration in Promotion console, store availability and redemption in Store management console, browsing and
checkout in Customer ordering website, and order validation, stock reservation and duplicate-request handling in
Restaurant order API. Mobile ordering app has browsing, order retry and paid-order collection passes; Staff ordering
tablet has table orders, meal-deal extras and revision conflicts; Checkout register has server quotes, payment and
redemption confirmation; Kitchen display has station tickets, printer-job status and food-ready updates. Each
repository includes tests for its own role; frontend repositories also include components, API clients, domain rules
and web configuration, while Restaurant order API includes HTTP validation, database queries and migrations.

Files can be opened in the real viewer, and the source shown there agrees with the corresponding diff. All sample
repositories exist only in memory; their displayed source is not executed. The examples do not provide working
restaurant services, payment devices or printers.

Selecting the coordinator and opening its report panel shows a progress report in every scene. The report separates
each project's implemented results from its ongoing integration checks or pending feedback. It presents sample
figures of eight coordinated projects, 48 checks passed so far and one decision
awaiting confirmation: whether the meal deal can be combined with a member discount. The sample keeps the discounts
separate while other implementation and integration tasks continue. The code-review scene shows
`src/meal-deal-orders.ts` on the Restaurant order API project, with changes that calculate prices from an eligible
active offer, reserve available stock and prevent repeated requests from creating duplicate orders, alongside sample
tests and promotion rules.
These files and check counts are demonstration data.

Every agent counts as installed in the preview, and the rail's account quota panel shows sample readings of each agent's
default account: Claude Code on the Max plan and Codex on the Plus plan, each with a five-hour and an account-wide
weekly window, and Grok Build on SuperGrok with a weekly credit pool. Their reset times count from when the preview
loaded, so no visit finds a reset already past. These readings are demonstration data.

The homepage embeds a complete desktop viewport scaled to fit its available width. The preview reserves its final
responsive height while loading, keeping section-link destinations stable as it initializes. The application fills the
embedded viewport, with the sample-data explanation in a caption below it. Once ready, the embedded
application accepts pointer and keyboard interaction, including its report and project panes, and is available to
assistive technology. While loading, it remains hidden and cannot receive interaction. A continuous border with
four rounded corners frames the preview. At rest, its lower edge, including both application content and border,
fades into the page. Hovering over the preview removes the fade and reveals the complete border. After a pointer
click, moving the pointer outside restores the fade even if a control inside still has focus. Keyboard interaction
keeps the preview unfaded while focus remains inside it, including inside the report panel. The fade leaves focus,
text entry, selection and scrolling intact. Devices without hover show no fade.
The standalone link opens the selected scene and language in a new tab at `preview/website-preview.html`, relative
to the site's deployment root. The standalone preview displays its sample-data label above the application.

The embedded preview never takes keyboard focus by itself. While focus is elsewhere on the homepage, the application's
own focus moves, such as its agent terminal focusing itself as the preview loads or the steps that set up a scene,
leave focus where it is. Focus enters the preview only when the visitor tabs into it or presses (clicks or taps)
inside it; a press inside gives the preview focus as a click in any frame does, and from then on the application moves
focus inside it as usual. Tab and Shift+Tab, with or without Option, move through the preview's controls and, from its
first or last control, leave it for the homepage's previous or next control, so keyboard visitors can reach the rest of
the homepage, including the footer and the analytics consent prompt. This holds in the preview's terminals too, where
Tab moves focus instead of reaching the terminal as it does in the application (see "Typing and the mouse" in
`docs/product/session-terminal.md`), and in the code-review scene's diff viewer, an open modal dialog, where Tab at
its first or last control leaves the preview instead of wrapping around within the dialog. A menu or list box the
visitor opens inside the preview keeps the application's Tab behavior, and Escape closes it. Tab combined with Ctrl or
Meta keeps its application meaning. The standalone preview keeps the application's own focus and Tab behavior.

On devices whose primary pointer is touch, both previews keep text-entry fields read-only so tapping or automatically
focusing the terminal or another text field does not open the software keyboard. Buttons, menus, selection and scrolling
remain interactive. Devices whose primary pointer is a mouse or trackpad retain keyboard text entry, including
touchscreen desktops. This restriction applies only to the website previews, not to the application itself.

Vertical mouse-wheel and trackpad gestures scroll the embedded list, terminal, code or report under the pointer
while it has room to scroll in that direction. At its top or bottom boundary, continuing in the same direction
scrolls the surrounding homepage. Horizontal gestures, gestures modified by Shift, Ctrl or Meta, and zoom retain
their normal behavior. The standalone preview keeps native scrolling within the application.

On touch screens the same handoff applies to a one-finger vertical drag, decided once per gesture from its first
movement: if the embedded list, terminal, code or report can scroll in that direction, the drag scrolls it and never
the homepage; otherwise the whole drag scrolls the homepage. Mostly horizontal drags, pinches and drags that start on
a control that handles touches itself, such as a resize handle, are left alone. When a drag that scrolled the
homepage ends while the finger is still moving, the homepage keeps gliding and slows to a stop, as native scrolling
does; touching the screen again, using the wheel, reaching the top or bottom of the page or leaving the page ends the
glide. Visitors who prefer reduced motion get no glide.

When the embedded preview's container is 800 pixels wide or narrower, the application uses that width in a
640-pixel-high viewport. Both the embedded and standalone previews support the application's responsive layout;
on narrow screens the sidebar is a drawer, while the report, Files and Terminal panels dock beside the agent terminal,
or, on phones, float over it one at a time and close with a tap beside them. The overview scene shows the agent terminal
with no panel open. On phones, the focus-mode scene initially floats the Terminal panel, and visitors can switch to
Files; switching floating panels leaves the desktop layout with both panels open. The session-list toggle opens the
project's focus cards. The code-review scene opens the Files panel and the sample
change in the diff viewer automatically, including on phones. The preview does not connect to a daemon, run real
agents, read or change real project files, or retain its preferences after a reload.
It does not replace the website's stored language preference. The application controls follow the selected supported
UI language; the fictional tasks, the agents' prose and task prompts in the terminals, and the report use Simplified
Chinese for that language and English for all others. What the agents' tools print in the terminals, such as line
counts, test results and key hints, stays in English in every language.

The embedded view shows a loading message until ready. A reported startup failure, or no readiness after 20 seconds,
shows a localized preview-specific error message that directs the visitor to choose a view above to retry. Selecting
another scene or the current scene again starts a new loading attempt. The standalone preview shows an alert if its
startup fails. JavaScript is required to load and operate either preview.
