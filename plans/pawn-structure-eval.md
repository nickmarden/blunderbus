# Plan: Pawn Structure Evaluation

## Goal

Penalize weak pawn structures — specifically doubled pawns (two friendly
pawns on the same file) and isolated pawns (a pawn with no friendly pawns
on adjacent files). These are the two most common structural weaknesses and
are detectable with simple file-mask bitboard arithmetic.

## What we are NOT doing yet

- Backward pawns (pawn that cannot advance without becoming a target)
- Connected pawn bonuses
- Pawn islands (count of disconnected pawn groups)
- Interaction with passed pawns (separate plan)

---

## Step 1: Doubled pawn detection

Two or more pawns on the same file are doubled. The penalty applies once
per extra pawn beyond the first on a file (so three pawns on a file = two
penalties).

```rust
fn doubled_pawn_penalty(pawns: Bitboard) -> i32 {
    let mut penalty = 0;
    for file in 0..8 {
        let mask = file_mask(file);
        let count = (pawns & mask).popcount();
        if count > 1 {
            penalty += (count - 1) as i32 * DOUBLED_PAWN_PENALTY;
        }
    }
    penalty
}
```

`file_mask(file)` already exists (or will exist) from the king safety plan:
`Bitboard(0x0101_0101_0101_0101u64 << file)`.

**Starting penalty value:** -20 cp per extra pawn on a file.

---

## Step 2: Isolated pawn detection

A pawn on file F is isolated if there are no friendly pawns on files F-1
or F+1 (on any rank). Detect with adjacent file masks:

```rust
fn isolated_pawn_penalty(pawns: Bitboard) -> i32 {
    let mut penalty = 0;
    for file in 0..8 {
        let on_file = pawns & file_mask(file);
        if on_file.is_empty() {
            continue;
        }
        let left  = if file > 0 { pawns & file_mask(file - 1) } else { Bitboard::EMPTY };
        let right = if file < 7 { pawns & file_mask(file + 1) } else { Bitboard::EMPTY };
        if left.is_empty() && right.is_empty() {
            // Every pawn on this file is isolated; penalize each one
            penalty += on_file.popcount() as i32 * ISOLATED_PAWN_PENALTY;
        }
    }
    penalty
}
```

**Starting penalty value:** -15 cp per isolated pawn.

---

## Step 3: Integration into eval.rs

Add `pawn_structure_penalty(pos: &Position, color: Color) -> i32`. Call
both helpers, sum the results (both return non-positive values).

In `evaluate()`, add:

```rust
score += sign * pawn_structure_penalty(pos, color);
```

Called once per color, same pattern as all other eval terms.

**Implementation note:** both helpers operate only on the pawn bitboard for
one color — `pos.bbs.pieces(color, PieceKind::Pawn)`. No board reads needed.

---

## Step 4: Tests to write

1. **No structural weaknesses** — staggered pawns on different files, none
   adjacent to each other. Penalty should be 0.

2. **Doubled pawns detected** — two White pawns on the e-file. Penalty
   equals exactly one `DOUBLED_PAWN_PENALTY`.

3. **Tripled pawns** — three White pawns on the e-file. Penalty equals two
   `DOUBLED_PAWN_PENALTY` values.

4. **Isolated pawn detected** — White pawn on a5, no White pawns on b-file.
   Penalty equals one `ISOLATED_PAWN_PENALTY`.

5. **Doubled AND isolated** — two White pawns on the a-file, no White pawns
   on b-file. Both penalties apply and stack.

6. **Starting position evaluates to 0** — pawns are symmetric, penalties
   cancel at the aggregate level.

7. **Black structural weakness penalized** — Black has a doubled pawn;
   result should be positive (White-favoring).

---

## Implementation order

1. Write the tests (they will fail)
2. Confirm `file_mask()` exists in `bitboard.rs` (add if not already there
   from the king safety plan)
3. Implement `pawn_structure_penalty()` in `eval.rs`
4. Make the tests pass
5. Run `bench.py --games 10 --depth 4` and compare ACPL to baseline
6. Tune penalty values if needed
7. Update CLAUDE.md

---

## Success criteria

- All existing tests still pass
- New pawn structure tests pass
- Starting position still evaluates to 0
- Engine avoids creating doubled/isolated pawns in self-play at depth 4+
