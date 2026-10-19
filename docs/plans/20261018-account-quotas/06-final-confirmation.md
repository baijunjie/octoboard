# 06 Final confirmation

> Goal: confirm in the running desktop app, against the real daemon, what the gallery's mock data and Chromium could
> not show.
> Completion criteria: every item below is checked in the packaged or dev desktop app on macOS and found as described
> in the product docs; any defect found is fixed here.

## Implementation plan

- [ ] Settings → Agent accounts shows real plans and quota windows read by the daemon for the default and owned
      accounts of each installed agent, including the loading state right after the daemon starts.
- [ ] In the desktop app's WebKit view: the agent tab strip stays pinned while a long account list scrolls, with the
      top fade below it; switching tabs plays no list motion; removing an account animates.
- [ ] A help tip (the "?" on Settings → Git → Automatically pull repositories) opens its popover on a trackpad tap
      and on a touch tap where available.
- [ ] Creating an account with the default account's directory, written as `~/…` and as the absolute path, is
      refused once the daemon has determined the agent's default directory.
- [ ] In the desktop app's WebKit view, the rail's account quota panel floats in on hover beside the rail above
      Settings, plays its open and close motion, and stays within the window at the narrowest window width.
- [ ] With a long account list in the rail panel, Tab reaches the "Accounts" list and it shows its focus ring; Tab and
      Shift+Tab out of a keyboard-pinned panel close it and move on to the control after or before the quota button.
- [ ] In the installed app, the menu bar icon's quota submenus, opened from the icon itself, show a Claude Code
      account's real quota, and their rows read correctly in both a light and a dark menu, including after switching
      the system appearance while the app runs; a row never highlights under the pointer, and a long window name or
      account widens the submenu instead of being cut.
- [ ] Opening the menu bar icon's menu refreshes an account whose reading is due, and the new reading shows on the
      next opening.
- [ ] VoiceOver reads a quota row of the menu bar icon's menu once, as its whole phrase.
