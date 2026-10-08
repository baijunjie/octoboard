# Read-only code and image viewer

> Goal: display a file in a modal with friendly code highlighting or image rendering, without editing.
> Completion criteria: representative common source files and images open in the viewer in both the browser and
> packaged desktop client; file content cannot be edited; the modal closes and restores focus correctly. This can be
> verified with supplied file contents independently of the project's default tree source.

## Technical design

- [ ] The viewer accepts the selected file's identity and content, distinguishing source/text content from images.
- [ ] Code content uses a reusable read-only renderer that covers mainstream source languages.
- [ ] Images are rendered inside the same modal viewer.
- [ ] The viewer exposes no edit or save interaction.

## Implementation plan

- [ ] Integrate the code renderer with the shared modal infrastructure and verify representative source languages.
- [ ] Integrate image presentation with that modal and verify the supported image formats.
- [ ] Verify switching between code and image content while keeping the same viewer open.
- [ ] Verify keyboard access, closing and focus restoration, light/dark appearance, and the browser and desktop builds.

## Notes for the developer

**Reusable capabilities**

- The UI dialogs module supplies the shared HeroUI modal frame, focus containment, closing, and refocus behavior.
- The UI theme and internationalization modules supply appearance and localized controls.
- The UI gallery supplies isolated scenarios for visual verification.

**Development notes**

- Library candidates and bundling/CSP constraints are in the overview. The library choice is not yet settled.
- The existing shared dialog supports small/medium/large sizes; evaluate an appropriate file-viewer size rather than
  assuming the current form-dialog sizes are sufficient.
- Unknown text languages should remain inspectable even if highlighting is unavailable; unsupported binary content
  needs an explicit presentation rather than being decoded as source text.

**Reference docs**

- `packages/ui/README.md`
- `docs/memory/writing-ui-components.md`
