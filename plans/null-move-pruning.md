# Plan: Null Move Pruning

## Goal

If we "pass" our turn (make a null move) and the resulting position still
causes a beta cutoff after a shallow search, the current position is almost
certainly too good for us to bother searching — prune it. This is one of the
most powerful search pruning techniques and can reduce the effective tree size
by 30-50% in the middlegame.

---

## The idea

At any non-root node, before searching our moves normally:

1. Make a "null move" — switch side to move without playing a piece.
2. Search the resulting position at reduced depth (`depth - 1 - R` where R=2).
3. If the score still exceeds beta, return beta (cutoff).

The intuition: if even giving the opponent a free tempo doesn't save them,
our position is so strong that the opponent can't escape a beta cutoff no
matter what we play. We don't need to find the exact best move.

---

## When NOT to null move

Null move pruning has well-known failure cases:

- **Zugzwang positions**: positions where passing hurts the passer (common
  in King+Pawn endgames). Guard by only applying null move when we have
  at least one non-pawn piece (bishop, knight, rook, or queen).
- **In check**: if we're in check, we cannot pass — we must move.
- **At depth <= 2**: too close to the horizon; null move reduces depth below
  quiescence.
- **Consecutive null moves**: never allow two null moves in a row (add a
  `last_was_null` flag to the call chain).

---

## Implementation sketch

In `negamax`, before the move loop:

```rust
let has_pieces = (pos.bbs.pieces(stm, Knight) | pos.bbs.pieces(stm, Bishop)
                | pos.bbs.pieces(stm, Rook)   | pos.bbs.pieces(stm, Queen))
                .popcount() > 0;

if depth >= 3 && !in_check && !last_was_null && has_pieces {
    let null_pos = make_null_move(pos);   // just flip side_to_move, clear en_passant
    let null_score = -negamax(&null_pos, depth - 1 - 2, -beta, -beta + 1,
                               ply + 1, nodes, history, qdepth,
                               tt, killers, /*last_was_null=*/true);
    if null_score >= beta {
        return beta;  // cutoff
    }
}
```

The reduction `R = 2` is standard. Adaptive R (R=3 at high depth) is a
later refinement.

### `make_null_move`

```rust
fn make_null_move(pos: &Position) -> Position {
    Position {
        side_to_move: pos.side_to_move.opposite(),
        en_passant: None,   // must clear — en passant can't be carried over
        halfmove_clock: pos.halfmove_clock + 1,
        // everything else unchanged
        ..pos.clone()
    }
}
```

The hash needs updating: XOR out the en passant key (if any) and XOR in the
black-to-move key. Recompute via `compute_hash()` for correctness first;
incremental update is a later optimization.

---

## Tests to write

1. A clearly winning position does not get pruned incorrectly (score still correct).
2. Two consecutive null moves are never made.
3. Null move is not tried when in check.
4. Null move is not tried when only king and pawns remain (zugzwang guard).
5. Node count at depth 6 is lower with null move than without on a tactical position.

---

## Success criteria

- All existing tests pass.
- Perft counts unchanged (null move only runs in `negamax`, not `perft`).
- Node count visibly reduced at depth 5+.
- No crashes or score distortions on king+pawn endgame positions.
