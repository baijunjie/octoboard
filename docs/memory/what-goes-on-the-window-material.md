# What goes on the window material

## Only icon-only chrome sits on the window material; text goes on the opaque panel

In the macOS app the top bar and the rail show the translucent window material (see "The window chrome and the
content panel" in `docs/product/window-layout.md`), whose colour shifts with whatever is behind the window. The user
rejected a layout that put the text-heavy sidebar on it, and one that floated the content as a card with gutters of
material around it. So nothing beyond an icon, an avatar or the breadcrumb's one line goes on the chrome; anything
else belongs in the opaque content panel. A glyph or tint that does go there is measured against the material itself,
not only against `--chrome` (the "A glyph or tint on the window material" section of `docs/ui-accessibility.md`).
