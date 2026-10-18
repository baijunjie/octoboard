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
- [ ] The default account's help tip opens its popover on a trackpad tap and on a touch tap where available.
- [ ] Creating an account with the default account's directory, written as `~/…` and as the absolute path, is
      refused once the daemon has determined the agent's default directory.
