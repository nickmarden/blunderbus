# Plan: King Safety Evaluation

## Goal

Add a king safety penalty to `eval.rs` to address the engine's systematic
failure to protect its own king. This is the most likely cause of the
White/Black ACPL asymmetry seen in benchmarking.

## What we are NOT doing yet

- Attacker proximity (counting enemy pieces near the king)
- Pawn storms
- Middlegame/endgame phase blending (separate plan)

Those can come later. Start with the two cheapest, highest-impact terms.

---

## Step 1: Pawn shield penalty

A castled king on g1/h1 (or g8/h8) expects pawns on f2/g2/h2 (or f7/g7/h7).
When those pawns are missing or advanced, the king is exposed.

**What to compute:**

- Find the king's square via `pos.bbs.pieces(color, King).lsb()`
- The "shield rank" is one rank in front of the king (toward the opponent)
- The shield squares are the king's file and the two adjacent files, on
  that rank
- For each of the three shield squares, check whether a friendly pawn is
  present on that square OR one rank further advanced (pawn pushed one step)

**Penalty values (starting point — tune after benchmarking):**

| Condition                          | Penalty |
|------------------------------------|---------|
| Shield pawn present on rank+1      |  0 cp   |
| Shield pawn advanced one rank      | -10 cp  |
| Shield pawn gone entirely          | -20 cp  |

Max pawn-shield penalty: -60 cp (all three pawns gone).

**Edge cases:**

- King on the a or h file: only two shield squares, not three
- King in the center (not castled): the pawn-shield concept doesn't apply
  well; only activate this penalty when the king is on ranks 1/2 (White)
  or ranks 7/8 (Black), i.e. still near the back rank

---

## Step 2: Open file near king penalty

An open file (no pawns of either color) pointing at the king lets rooks and
queens attack directly. A semi-open file (only enemy pawns) is a lesser
threat.

**What to compute:**

- For each of the three files centered on the king's file (clamped to a-h):
  - Is the file fully open? (no pawns of either color on that file)
  - Is it semi-open for the opponent? (no friendly pawns, but enemy pawns)

**Penalty values (starting point):**

| Condition                          | Penalty |
|------------------------------------|---------|
| Fully open file near king          | -25 cp  |
| Semi-open file near king           | -10 cp  |

Max open-file penalty: -75 cp (all three files open).

---

## Step 3: Integration into eval.rs

Add a `king_safety_penalty(pos: &Position, color: Color) -> i32` function
that returns the combined penalty as a non-positive number.

In `evaluate()`, add:

```rust
score += sign * king_safety_penalty(pos, color);
```

This is called once per color, same as all other eval terms.

---

## Step 4: Bitboard infrastructure needed

We currently only have FILE_A, FILE_B, FILE_G, FILE_H as named constants.
Rather than adding FILE_C through FILE_F, compute file masks dynamically:

```rust
fn file_mask(file: u8) -> Bitboard {
    Bitboard(0x0101_0101_0101_0101u64 << file)
}
```

No new constants needed.

---

## Step 5: Tests

Write tests in `eval.rs` before implementation:

1. **Castled king, full shield** — White king on g1, pawns on f2/g2/h2.
   Penalty should be 0.

2. **One pawn advanced** — White king on g1, g-pawn on g4 (two ranks
   advanced, shield broken). Should produce a penalty.

3. **Two pawns gone** — White king on g1, f2 and h2 pawns captured.
   Should produce a larger penalty.

4. **Open file toward king** — White king on g1, no pawns on g-file.
   Should add an open-file penalty on top of pawn-shield penalty.

5. **Starting position still evaluates to 0** — kings are symmetric,
   so penalties cancel and the net eval is unchanged.

6. **Directional sanity** — a position where Black's king is more
   exposed than White's should produce a positive (White-favoring) score.

---

## Implementation order

1. Write the tests (they will fail)
2. Add `file_mask()` helper to `bitboard.rs`
3. Implement `king_safety_penalty()` in `eval.rs`
4. Make the tests pass
5. Run `bench.py --games 10 --depth 4` and compare ACPL to baseline
6. Tune penalty values if the numbers move in the right direction
7. Update CLAUDE.md

---

## Success criteria

- All existing tests still pass
- New king safety tests pass
- `bench.py` shows reduced White/Black ACPL asymmetry at depth 4
  (Black median ACPL drops meaningfully from ~386)
- Starting position still evaluates to 0
