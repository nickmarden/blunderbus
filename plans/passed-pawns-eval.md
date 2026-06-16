# Plan: Passed Pawn Evaluation

## Goal

Award a bonus for passed pawns — pawns with no opposing pawn on the same
file or either adjacent file ahead of them. Passed pawns are winning threats
in the endgame and materially significant in the middlegame. This is one of
the highest-value positional terms after material.

## What we are NOT doing yet

- Promotion-square control (bonus for controlling the queening square)
- Connected passed pawns (double bonus for two adjacent passers)
- Rook behind passed pawn (separate plan)
- Endgame phase scaling of the bonus (separate plan)

---

## Step 1: Passed pawn detection

A pawn on file F, rank R is passed if the opponent has no pawns on files
F-1, F, or F+1 on ranks R+1 through 8 (for White; mirror for Black).

This is cleanly expressed as a bitboard fill operation:

```rust
fn passed_pawns(our_pawns: Bitboard, their_pawns: Bitboard, color: Color) -> Bitboard {
    // Build a mask of all squares "ahead" of each enemy pawn on its file
    // and adjacent files. Any friendly pawn under that mask is NOT passed.
    let span = enemy_front_fill(their_pawns, color);  // fill toward our back rank
    let span_wide = span | shift_east(span) | shift_west(span);
    our_pawns & !span_wide
}
```

The "front fill" walks the enemy pawn bitboard toward our side of the board
one rank at a time, accumulating all squares on the file ahead of each enemy
pawn. This is a standard bitboard technique using repeated directional shifts.

**Implementation note:** add `front_fill(bb: Bitboard, color: Color) -> Bitboard`
to `bitboard.rs`. For White's pawns, fill southward (toward rank 1); for
detecting passers, we fill enemy pawns toward White's side, which means
filling southward for Black's pawns and northward for White's pawns.

---

## Step 2: Rank-based bonus table

A passer on rank 7 (one step from queening) is worth far more than one on
rank 3. Use a simple table indexed by rank (0-based, from our perspective):

```rust
const PASSED_PAWN_BONUS: [i32; 8] = [0, 0, 10, 20, 35, 55, 80, 0];
// rank:                              1   2   3   4   5   6   7   8
// rank 1 impossible; rank 8 means promoted (not a pawn)
```

For White, rank index = `sq.rank()`. For Black, flip: rank index = `7 - sq.rank()`.

---

## Step 3: Integration into eval.rs

Add `passed_pawn_bonus(pos: &Position, color: Color) -> i32`. Iterate the
passed pawn bitboard with `pop_lsb`, sum the table lookup per square.

In `evaluate()`, add:

```rust
score += sign * passed_pawn_bonus(pos, color);
```

Called once per color, same pattern as all other eval terms.

---

## Step 4: Tests to write

1. **No passed pawns** — mirrored pawn structure, both sides blocked.
   Bonus should be 0 for both colors.

2. **Single White passer on rank 5** — no Black pawn on d-, e-, or f-file
   ahead of it. White should receive a positive bonus.

3. **Rank scaling** — same passer moved to rank 6 should give a larger bonus
   than rank 4.

4. **Black passer detected** — symmetric position with Black's passer;
   Black should receive the equivalent bonus (negative from White's view).

5. **Blocked passer still counts** — a passed pawn blocked by an enemy piece
   (not a pawn) is still passed.

6. **Adjacent enemy pawn on same rank does not block** — a Black pawn on d5
   does not block a White passer on e4 (it is beside, not ahead).

---

## Implementation order

1. Write the tests (they will fail)
2. Add `front_fill()` to `bitboard.rs`
3. Implement `passed_pawn_bonus()` in `eval.rs`
4. Make the tests pass
5. Run `bench.py --games 10 --depth 4` and compare ACPL to baseline
6. Tune bonus values if improvement is marginal
7. Update CLAUDE.md

---

## Success criteria

- All existing tests still pass
- New passed pawn tests pass
- Starting position evaluates to 0 (no passers, symmetric)
- Engine visibly advances passed pawns rather than shuffling pieces
