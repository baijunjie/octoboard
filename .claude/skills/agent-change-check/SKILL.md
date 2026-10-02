---
name: agent-change-check
description: Change check entry point — when wrapping up, dispatch the change-checker subagent to review this round of changes, then once you have agreed on its comments, make the fixes and have it review again until it passes. Use when the user says "review this round of changes", "wrap-up review", "check the changes I just made".
---

# Change check

1. **Dispatch the `change-checker` subagent to review this round of changes; do not review what you wrote
   yourself**: the context you are holding treats the structure you just wrote as settled fact. Its checklist and
   judgement criteria are in its own definition; you only hand over "what changed this round and what the requirement
   is", plus the trade-offs the user settled over several rounds of revision — it cannot see your conversation, and
   anything you leave out comes back reported as a problem.
2. **Work through the comments one by one**: fix the ones you agree with; explain your reasoning for the ones you do
   not, and when you cannot reach agreement with it, or a comment would overturn a trade-off the user settled, leave
   it to the user to decide — do not sign off yourself.
3. **Dispatch it again once the fixes are in**, until it passes. In your final reply, state how many rounds of review
   there were, what was changed, and which comments are left for the user to decide.

Tools without a subagent mechanism: read the installed `change-checker` definition and review it yourself against the
checklist in it, but you must first read every file involved in the change in full, not just the diff.

## When the change spans a lot

When the volume of changes is large and spans several relatively isolated projects or modules, **split by scope,
dispatch one `change-checker` per scope, and review in parallel**; do not let one subagent review all of it: once it
has too much full text to read it can only skim, and details get missed. Where scopes share an interface contract,
hand that contract to every side involved; when fixing comments touches a cross-scope interface, re-dispatch a review
for the scopes on both sides of it.
