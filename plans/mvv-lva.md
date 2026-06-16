# Plan: MVV-LVA Capture Ordering

## Goal

Sort captures so that the most promising ones are searched first. Alpha-beta
pruning works best when good moves are tried early, because each good move
raises alpha and prunes more of the remaining tree. Currently captures are
tried before quiet moves but in arbitrary order within captures.

MVV-LVA: Most Valuable Victim / Least Valuable Attacker. Capturing a queen
with a pawn (high victim, low attacker) is tried before capturing a pawn
with a queen (low victim, high attacker). The idea is that good captures
(net material gain) are more likely to be best and should be searched first.

---

## What changes

### In `src/movegen.rs` or `src/search.rs`

The existing `order_moves` function in `search.rs` does:

```rust
fn order_moves(pos: &Position, moves: &mut Vec<Move>) {
    moves.sort_by_key(|mv| if is_capture(pos, mv) { 0 } else { 1 });
}
```

Replace the sort key for captures with an MVV-LVA score:

```rust
fn mvv_lva_score(pos: &Position, mv: &Move) -> i32 {
    let victim_value = captured_piece_value(pos, mv);   // 0 if not a capture
    let attacker_value = moving_piece_value(pos, mv);
    // Higher = better; negate so sort_by_key ascending puts best first
    victim_value * 10 - attacker_value
}
```

The `* 10` multiplier ensures victim dominates: PxQ (100*10 - 900 = 100) >
NxQ (320*10 - 900 = 2300)... wait, reverse: we want PxQ first so score
should be higher for better captures. Use descending sort or negate.

Final ordering key (lower = searched first):

```
key = -(victim_value * 10 - attacker_value)
```

- PxQ: -(900*10 - 100) = -8900  (searched first — great capture)
- QxP: -(100*10 - 900) = -100   (searched later — risky capture)
- Non-capture: 0                 (searched last)

### Helper functions needed

```rust
fn captured_piece_value(pos: &Position, mv: &Move) -> i32
fn moving_piece_value(pos: &Position, mv: &Move) -> i32
```

Both look up the piece at the relevant square via `pos.bbs.piece_at()` and
call `material_value(kind)` from `eval.rs`. En passant is a capture with
no piece on `mv.to` — handle specially (victim = pawn).

---

## Tests to write

1. Captures sorted before quiet moves (existing behavior preserved).
2. PxQ sorted before QxP in the same position.
3. Equal captures (PxP, NxN) are stable relative to each other.
4. En passant capture included in capture ordering.

---

## Success criteria

- All existing tests pass (perft counts unchanged — ordering doesn't affect correctness).
- `search::tests::tt_improves_node_count` still passes and ideally the node
  count is lower with MVV-LVA ordering (fewer nodes needed at same depth).
- Engine plays noticeably more tactically sound captures in self-play.
