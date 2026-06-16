# Plan: Rook Open File Evaluation

## Goal

Reward rooks placed on open and semi-open files, and on the 7th rank. These
are the most important positional bonuses for rooks and have an outsized
effect on engine play quality — a rook on a closed file is nearly inert,
while a rook on an open file dominates a column and ties down the opponent.

## What we are NOT doing yet

- Rook behind a passed pawn (requires passed pawn detection; separate plan)
- Connected rooks (two rooks on the same open file)
- Doubling rooks on the 7th rank

---

## Step 1: Classify each rook's file

For each rook, check whether its file contains pawns:

```rust
fn file_is_open(file: u8, all_pawns: Bitboard) -> bool {
    (all_pawns & file_mask(file)).is_empty()
}

fn file_is_semi_open(file: u8, friendly_pawns: Bitboard) -> bool {
    (friendly_pawns & file_mask(file)).is_empty()
}
```

- **Fully open:** no pawns of either color on the file. Best bonus.
- **Semi-open:** no friendly pawns, but enemy pawns present. Smaller bonus.
- **Closed:** friendly pawns on the file. No bonus.

**Starting bonus values:**

| Condition         | Bonus  |
|-------------------|--------|
| Fully open file   | +20 cp |
| Semi-open file    | +10 cp |

`file_mask(file)` is the same helper used in the king safety and pawn
structure plans: `Bitboard(0x0101_0101_0101_0101u64 << file)`.

---

## Step 2: 7th rank bonus

A rook on the 7th rank (rank 7 for White, rank 2 for Black) attacks the
opponent's pawns on their starting squares and often traps the enemy king
on the back rank.

```rust
fn rook_on_seventh(rook_sq: Square, color: Color) -> bool {
    let seventh = match color {
        Color::White => 6,  // rank index 6 = rank 7
        Color::Black => 1,  // rank index 1 = rank 2
    };
    rook_sq.rank() == seventh
}
```

**Starting bonus value:** +25 cp per rook on the 7th rank.

The 7th rank bonus stacks with the open file bonus — a rook on an open file
pointing at the 7th rank is doubly well-placed.

---

## Step 3: Integration into eval.rs

Add `rook_bonus(pos: &Position, color: Color) -> i32`. Iterate the rook
bitboard with `pop_lsb`, classify each rook's file and rank, accumulate.

```rust
fn rook_bonus(pos: &Position, color: Color) -> i32 {
    let mut bonus = 0;
    let friendly_pawns = pos.bbs.pieces(color, PieceKind::Pawn);
    let all_pawns = friendly_pawns | pos.bbs.pieces(color.opposite(), PieceKind::Pawn);
    let mut rooks = pos.bbs.pieces(color, PieceKind::Rook);
    while !rooks.is_empty() {
        let sq = rooks.pop_lsb();
        let file = sq.file();
        if file_is_open(file, all_pawns) {
            bonus += ROOK_OPEN_FILE_BONUS;
        } else if file_is_semi_open(file, friendly_pawns) {
            bonus += ROOK_SEMI_OPEN_FILE_BONUS;
        }
        if rook_on_seventh(sq, color) {
            bonus += ROOK_SEVENTH_RANK_BONUS;
        }
    }
    bonus
}
```

In `evaluate()`, add:

```rust
score += sign * rook_bonus(pos, color);
```

---

## Step 4: Tests to write

1. **Rook on closed file** — friendly pawn on same file. Bonus should be 0.

2. **Rook on semi-open file** — no friendly pawn, enemy pawn on same file.
   Bonus equals `ROOK_SEMI_OPEN_FILE_BONUS`.

3. **Rook on fully open file** — no pawns of either color on the file.
   Bonus equals `ROOK_OPEN_FILE_BONUS`.

4. **Rook on 7th rank (White)** — White rook on e7. Gets the 7th rank bonus
   regardless of file status.

5. **Rook on 7th rank AND open file** — both bonuses stack.

6. **Black rook on 2nd rank** — symmetric to White's 7th rank case.

7. **Starting position evaluates to 0** — all rook files have friendly pawns,
   no 7th rank rooks, bonuses cancel symmetrically.

---

## Implementation order

1. Write the tests (they will fail)
2. Confirm `file_mask()` exists in `bitboard.rs`
3. Implement `rook_bonus()` in `eval.rs`
4. Make the tests pass
5. Run `bench.py --games 10 --depth 4` and compare ACPL to baseline
6. Tune bonus values if needed
7. Update CLAUDE.md

---

## Success criteria

- All existing tests still pass
- New rook bonus tests pass
- Starting position still evaluates to 0
- Engine reliably opens files for rooks and centralizes rooks in self-play
