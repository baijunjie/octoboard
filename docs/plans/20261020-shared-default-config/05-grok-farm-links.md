# 05 Grok session homes over shared entries

> Goal: a Grok Build session of a sharing account saves a shared file without replacing the account's link.
> Completion criteria: the per-session home Octoboard builds for Grok Build links each entry to its fully resolved
> target, so a sharing account's entry is reached through one link; a real Grok Build saving a shared file
> (`AGENTS.md`, `pager.toml`, `disabled-hooks`) from a sharing account is measured against scratch directories and
> leaves the default directory's file updated and the account's link in place.

## Technical design

- [ ] The per-session home links an entry to the resolved target of the account's entry, not to the account's entry.

## Implementation plan

- [ ] Change the home's linking and measure Grok Build's saves through it.

## Handoff

- The home copies `bundled/`, `bundles/`, `hooks-paths` and `sandbox.toml` on every launch (Grok's
  sandbox startup check refuses them as links), following a linked top-level entry, and copies `config.toml` and
  `trusted_folders.toml`; those stay copies. Resolving to the target applies only to the entries the home still links.
- Until this lands, a Grok Build save that follows only one link of the two-level chain (home entry → account entry →
  default entry) can replace the account's link with a plain file; the repair before the next launch backs it up and
  links it again.
- Because the home holds `config.toml` as a copy, check whether a Grok Build session saving it (turning a plugin on,
  say) reaches the account's file, and through its link the default directory's; if it does not, a sharing account's
  saves are lost, and the overview's premise that Grok saves `config.toml` through the link does not hold for
  sessions Octoboard launches.
