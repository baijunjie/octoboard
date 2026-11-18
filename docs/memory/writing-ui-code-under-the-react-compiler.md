# Writing UI code under the React Compiler

## Run `pnpm compiler:bailouts` on the files you changed, and read its list, not its exit status

The compiler rules and the requirement to leave none of your functions uncompiled are in `docs/conventions/ui-code.md`,
"The React Compiler". Pass the script the files you changed: with no files it checks all of `src/`, where functions
outside your change may already be listed, so its exit status over the whole tree says nothing about your change. It
prints each skipped function's line and the compiler's reason.
