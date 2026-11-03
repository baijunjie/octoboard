// What the browser's two trees (`FileTree`, `ChangeTree`) work out of their nodes: where a key finds
// its node, which keys are on screen, and how those nest for the list motion. Pure, so the trees
// differ only in which of their nodes are open.
import type { Key } from "react-aria-components";

export interface CollectionIndex<N> {
  /** Every node by its key, those under a closed node included. */
  byKey: Map<Key, N>;
  /** The keys of the rows on screen: the top-level nodes and what is under every open node. */
  visibleKeys: Set<string>;
  /** Each key on screen under its parent's, and the parents whose rows are shown (`ListFolds`). */
  folds: { parents: Map<string, string>; open: Set<string> };
}

/** An index of nothing, for `collectionIndex` to add to. */
export function emptyCollectionIndex<N>(): CollectionIndex<N> {
  return { byKey: new Map(), visibleKeys: new Set(), folds: { parents: new Map(), open: new Set() } };
}

/**
 * The index of `nodes` and everything under them (`childrenOf`); a node's children are on screen
 * where the node is and `isOpen` says it is open. With `under`, `nodes` are the rows of the
 * on-screen row `parent` and are added to the index `into`, which a tree made of several lists of
 * nodes (its sections) builds one list after another.
 */
export function collectionIndex<N extends { key: string }>(
  nodes: readonly N[],
  childrenOf: (node: N) => readonly N[],
  isOpen: (node: N) => boolean,
  under?: { parent: string; into: CollectionIndex<N> },
): CollectionIndex<N> {
  const index = under?.into ?? emptyCollectionIndex<N>();
  const walk = (list: readonly N[], parent: string | undefined, shown: boolean) => {
    for (const node of list) {
      index.byKey.set(node.key, node);
      if (shown) {
        index.visibleKeys.add(node.key);
        if (parent !== undefined) index.folds.parents.set(node.key, parent);
      }
      const open = shown && isOpen(node);
      if (open) index.folds.open.add(node.key);
      walk(childrenOf(node), node.key, open);
    }
  };
  walk(nodes, under?.parent, true);
  return index;
}
