# Previous and next file navigation

> Goal: move through files in directory-tree order without closing the modal.
> Completion criteria: for a supplied ordered file list containing code and images, previous/next buttons and
> left/right arrow keys select the same adjacent files in top-to-bottom order, and the viewer stays open while its
> subject changes. Opening and closing the modal does not leak navigation keys to the terminal or lose focus.

Depends on [the read-only viewer](01-read-only-viewer.md).

## Technical design

- [ ] The navigation context carries the ordered file list from the directory tree and the current file's identity.
- [ ] Previous and next buttons move backward and forward through that same order.
- [ ] Left and right arrow keys invoke previous and next respectively while operating the viewer.
- [ ] A file change updates the existing modal's subject and displays its code or image content.

## Implementation plan

- [ ] Connect the viewer to an ordered file-list context without independently re-sorting that list.
- [ ] Make button and keyboard navigation share the same previous/next behavior.
- [ ] Verify mixed code/image navigation, first/last boundaries under the chosen boundary policy, and modal focus
  while changing its subject.

## Notes for the developer

**Reusable capabilities**

- The UI dialogs module has reset-key and refocus mechanisms for changing a modal's subject without remounting it.
- The UI components module supplies named controls and tooltips.

**Development notes**

- The tree source, sorting, collapsed-directory inclusion and boundary policy remain open in the overview.
- Keep shortcuts local to the active viewer. Preserve arrow behavior for text selection and controls that consume
  arrow keys; account for composition and modifiers. These interactions need browser and WebKit verification.
- If files are loaded asynchronously, discard an older response after navigation changes the selected file; a failed
  or removed file must not silently show the previous file's contents under its name.

**Reference docs**

- `docs/memory/writing-ui-components.md`
- `docs/product/window-layout.md`
