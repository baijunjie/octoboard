> Severity: Moderate

# The import review and result give no values for composite application settings

- **Symptom**: In the configuration import review and the import result, the lines for Git workflow, Claude
  compatibility, Agent tags and Keyboard shortcuts show only a badge ("Set to the file's" / "Same" in the review,
  "Matches the file" in the result), so the user cannot tell what those settings will be, or were, changed to.
- **Reproduction steps**:
  1. In `packages/ui`, run `pnpm gallery --port <free port>`.
  2. Open `http://localhost:<port>/gallery.html?scenario=configuration-import-review-settings&theme=light&lang=en`.
  3. Look at the Git workflow, Claude compatibility, Agent tags and Keyboard shortcuts lines at the end of the
     Application settings list.
  4. Open `...?scenario=configuration-import-result-settings&theme=light&lang=en` and look at the same lines.
- **Expected vs. actual**:
  - Expected: each of these lines shows the concrete values the import changes (for example which agent gets which
    tags, which Git workflow settings change to what), so the user can judge the change as they can a switch's On/Off
    (position the user confirmed on the spot, 2026-11-02).
  - Actual: only "Set to the file's" / "Same" / "Matches the file", with no values; the user also could not tell why
    these four stand as separate lines among single switches.
- **Environment**: any platform; a configuration file whose application settings carry values for these four settings.
- **Scope of impact**: every user importing application settings; workaround: open the exported JSON file, or compare
  Settings after the import.
- **Leads**:
  - Verified: the lines are rendered by `packages/ui/src/configuration/ImportSettings.tsx` (review) and
    `ImportResult.tsx` (result); the review gets the file's and the current values (`InspectedSettings`, e.g.
    `agent_tags` and `current_agent_tags`), so the values exist on the UI side.
  - Verified: `docs/product/configuration-export-and-import.md` ("The application settings an export carries") gives
    each of these settings one line in the review and the result.
- **Acceptance criteria**:
  - [ ] The review line of each of Git workflow, Claude compatibility, Agent tags and Keyboard shortcuts shows what the
        import would change, with the values.
  - [ ] The result line of each shows what was applied, with the values.
  - [ ] A setting the import leaves unchanged still reads as unchanged.
  - [ ] The product doc and the gallery scenarios `configuration-import-review-settings` and
        `configuration-import-result-settings` show the new lines.
