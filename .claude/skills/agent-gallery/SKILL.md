---
name: agent-gallery
description: Entry point for the UI state gallery — dispatch the gallery-writer subagent for any writing or change to the gallery, and iterate on what it hands back. Use when a change adds or alters a UI state, when the user says "add a gallery scenario", "show this state in the gallery", "the gallery is out of date", "clean up the gallery".
---

# UI state gallery

1. **Dispatch the `gallery-writer` subagent for every change to the gallery; write no part of it yourself.** It
   decides how a state is best shown, and it cannot see your conversation: hand over the UI states this round
   produced or changed, what each one is for, and the trade-offs the user settled.
2. **One dispatch covers the whole round's gallery work**, so it can judge across scenarios which to extend and which
   to delete; several at once would each add their own.
3. Work through what it hands back: act on the states it reports as unreachable, and leave to the user any judgement
   it could not settle.
4. When unsure, report back.
