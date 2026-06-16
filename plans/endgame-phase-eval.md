# Plan: Endgame Phase Detection and King Table Blending

## Goal

Detect the game phase from remaining material and use it to blend between
the existing middlegame king table and a new endgame king table. The current
`KING_TABLE` pushes the king to the back rank, which is correct in the
middlegame but wrong in the endgame — an endgame king should be active and
centralized.

## What we are NOT doing yet

- Full tapered eval across all piece tables (extends this approach to pawns,
  knights, etc.; a larger separate effort)
- Separate mop-up evaluation (driving the losing king to a corner)
- Detecting specific endgame types (KP vs K, KR vs K, etc.)

---

## Step 1: Phase score computation

The phase score runs from 0 (opening/middlegame) to 256 (pure endgame).
It is computed from the non-pawn, non-king material still on the board.

Assign each piece type a phase weight:

```rust
const PHASE_WEIGHTS: [i32; 6] = [
//  Pawn  Knight  Bishop  Rook  Queen  King
    0,    1,      1,      2,    4,     0
];
const TOTAL_PHASE: i32 = 24; // 4*1 + 4*1 + 4*2 + 2*4 = 24 at game start
```

Compute:

```rust
fn game_phase(pos: &Position) -> i32 {
    let mut phase = TOTAL_PHASE;
    for color in [Color::White, Color::Black] {
        for kind in [Knight, Bishop, Rook, Queen] {
            let count = pos.bbs.pieces(color, kind).popcount() as i32;
            phase -= count * PHASE_WEIGHTS[kind as usize];
        }
    }
    // Clamp to [0, TOTAL_PHASE], then scale to [0, 256]
    let phase = phase.clamp(0, TOTAL_PHASE);
    (phase * 256 + TOTAL_PHASE / 2) / TOTAL_PHASE
}
```

When all pieces are on the board, `phase` returns 0 toward 256 as pieces
come off. This matches the standard "phase" convention used in most engines.

---

## Step 2: Endgame king table

Add a second king table alongside the existing `KING_TABLE` (which becomes
`KING_MG_TABLE`):

```rust
// Endgame king: wants to be central, not on back rank
const KING_EG_TABLE: [i32; 64] = [
    -50,-30,-30,-30,-30,-30,-30,-50,
    -30,-20, -5, -5, -5, -5,-20,-30,
    -30, -5, 20, 25, 25, 20, -5,-30,
    -30, -5, 25, 30, 30, 25, -5,-30,
    -30, -5, 25, 30, 30, 25, -5,-30,
    -30, -5, 20, 25, 25, 20, -5,-30,
    -30,-20, -5, -5, -5, -5,-20,-30,
    -50,-30,-30,-30,-30,-30,-30,-50,
];
```

Written rank 8 (top) to rank 1 (bottom), same convention as the existing
tables in `eval.rs`.

---

## Step 3: Interpolation in piece_square_bonus

Change `piece_square_bonus` to accept the phase score and blend the two
king tables. All other piece tables are unaffected for now.

```rust
fn piece_square_bonus(kind: PieceKind, color: Color, sq: Square, phase: i32) -> i32 {
    // ... existing rank-flip logic unchanged ...
    let mg = KING_MG_TABLE[idx]; // was KING_TABLE
    let eg = KING_EG_TABLE[idx];
    // linear interpolation: phase 0 = pure MG, phase 256 = pure EG
    (mg * (256 - phase) + eg * phase) / 256
}
```

Only the King branch changes. All other piece kinds return their table value
unchanged.

**Rust note:** `evaluate()` computes `phase` once at the top, then threads
it into `piece_square_bonus`. This means the function signature changes; fix
all call sites (there should only be one call site inside `evaluate()`).

---

## Step 4: Tests to write

1. **Middlegame phase score** — starting position has all pieces; phase
   should be 0 (or very close).

2. **Pure endgame phase score** — position with only kings and pawns; phase
   should be 256.

3. **Partial endgame** — queens traded, some minor pieces remain; phase
   should be between 0 and 256.

4. **King centralization rewarded in endgame** — king on e4 vs e1 should
   score higher in a pure endgame position.

5. **King on back rank rewarded in middlegame** — king on g1 (castled) vs e4
   should score higher in a full-piece position.

6. **Starting position still evaluates to 0** — symmetric positions must
   still cancel correctly after the table change.

---

## Implementation order

1. Write the tests (they will fail)
2. Rename `KING_TABLE` to `KING_MG_TABLE` in `eval.rs`
3. Add `KING_EG_TABLE`
4. Add `game_phase()` function
5. Update `piece_square_bonus` signature to accept `phase: i32`
6. Implement the interpolation for the King case
7. Thread `phase` through `evaluate()` to the call site
8. Make the tests pass
9. Run `bench.py --games 10 --depth 4` and compare ACPL to baseline
10. Update CLAUDE.md

---

## Success criteria

- All existing tests still pass
- New phase/king tests pass
- Starting position still evaluates to 0
- Engine king visibly centralizes in endgame positions during self-play
- `game_phase()` returns 0 at start and 256 with only kings+pawns remaining
