# Plan: Killer Move Heuristic

## Goal

When a quiet move causes a beta cutoff (it's "too good" for the opponent to
allow), remember it. At the same ply in sibling nodes, try it early — it
often works there too, since the same tactical threat exists on the same move
number regardless of the exact position.

This is the most widely used move ordering heuristic after MVV-LVA. It is
cheap to implement and typically reduces the node count by 10-20%.

---

## What are killer moves?

At each ply of the search tree, we keep two slots: `killers[ply][0]` and
`killers[ply][1]`. When a quiet move causes a beta cutoff, we shift the old
killer to slot 1 and store the new move in slot 0. On the next node at the
same ply, we try killer[0] and killer[1] before the remaining quiet moves.

Killers are keyed by ply (depth from root), not by position hash. They are
not transposition-table entries — they do not need to be correct for every
position, just likely to be good at this depth in this search.

---

## Data structure

Add to `search.rs`:

```rust
const MAX_PLY: usize = 64;
type KillerTable = [[Option<Move>; 2]; MAX_PLY];
```

Pass `&mut KillerTable` through the negamax call chain. Reset to all `None`
at the start of each `search()` call (killers are search-local, not game-global).

---

## Where to update killers

In `negamax`, after a beta cutoff on a quiet move:

```rust
if score >= beta && mv.kind == MoveKind::Normal {
    // Store killer: shift old slot 0 to slot 1, new move to slot 0
    killers[ply][1] = killers[ply][0];
    killers[ply][0] = Some(mv);
    return beta;
}
```

Only quiet moves are killers. Captures are already ordered by MVV-LVA and
handled separately.

---

## Move ordering with killers

Updated `order_moves` priority:

1. Captures (sorted by MVV-LVA score)
2. Killer move slot 0 (if legal in this position)
3. Killer move slot 1 (if legal in this position)
4. All other quiet moves

"Legal in this position" means the killer move must be in the legal move list
for the current position. A simple `.contains()` check suffices.

---

## Function signature changes

`negamax` gains a `killers: &mut KillerTable` parameter. `negamax_root` and
`search` initialize and pass it through.

---

## Tests to write

1. Node count at depth 5 on a tactical position is lower with killers than without.
2. Killers from a previous game/search do not persist (reset each `search()` call).
3. A killer move that is illegal in the current position is not tried (no crash).

---

## Success criteria

- All existing tests pass.
- Perft counts unchanged (ordering doesn't affect correctness).
- Node count in `tt_improves_node_count` or a new benchmark is lower.
- Self-play games show tighter, more consistent tactical play.
