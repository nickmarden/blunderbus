# Plan: Mobility Evaluation

## Goal

Award a small bonus for each legal move a piece has. More moves = more active
piece = better position. Mobility is a cheap proxy for piece activity and
particularly valuable for knights (whose mobility swings a lot) and bishops
(whose diagonals can be long or blocked).

---

## What mobility measures

For each piece type, count how many squares it can move to (pseudo-legal,
not full legal move generation — we want speed). Bonus per extra square of
mobility above a baseline.

Alternatively, count the attacked squares for each piece (already computed
in attack detection). The distinction:

- **Attacked squares**: all squares the piece threatens (including own pieces,
  can't actually land there)
- **Available moves**: squares it can legally move to

Attacked squares are faster to compute (no need to filter own pieces) and
correlate well with mobility. Use that.

---

## Per-piece mobility weights

Not all pieces benefit equally from mobility:

| Piece  | Bonus per square | Notes |
|--------|-----------------|-------|
| Knight | 4 cp            | Mobility swings 2-8; very sensitive |
| Bishop | 3 cp            | Long diagonals vs blocked; sensitive |
| Rook   | 2 cp            | Already rewarded by open file bonus |
| Queen  | 1 cp            | So many squares that variance is low |
| King   | 0               | King mobility is a safety concern, not a bonus |
| Pawn   | 0               | Covered by pawn structure eval |

---

## Implementation

Add `mobility_bonus(pos: &Position, color: Color) -> i32` in `eval.rs`.

For knights: use the precomputed `knight_attacks()` table — O(1) per knight.
For bishops/rooks/queens: walk rays (same logic as movegen), counting reachable
squares. A simplified version counts squares until blocked (friendly or enemy).

```rust
fn mobility_bonus(pos: &Position, color: Color) -> i32 {
    let occ = pos.bbs.occupancy();
    let friendly = pos.bbs.color_occupancy(color);
    let mut bonus = 0i32;

    // Knights
    let mut knights = pos.bbs.pieces(color, PieceKind::Knight);
    while !knights.is_empty() {
        let sq = knights.pop_lsb();
        let attacks = knight_attacks()[sq.index() as usize];
        bonus += (attacks & !friendly).popcount() as i32 * KNIGHT_MOBILITY_BONUS;
    }

    // Bishops
    let mut bishops = pos.bbs.pieces(color, PieceKind::Bishop);
    while !bishops.is_empty() {
        let sq = bishops.pop_lsb();
        let attacks = bishop_attacks(sq, occ) & !friendly;
        bonus += attacks.popcount() as i32 * BISHOP_MOBILITY_BONUS;
    }

    // Rooks and Queens similar...
    bonus
}
```

The `bishop_attacks(sq, occ)` ray-walk helper can be extracted from the
existing movegen slider logic.

---

## Tests to write

1. **Locked bishop vs open bishop**: a bishop with blocked diagonals scores
   less than one with open diagonals.
2. **Knight on rim vs center**: knight on a1 (2 moves) vs e4 (8 moves) —
   center knight should score higher.
3. **Starting position evaluates to 0**: both sides have equal mobility.
4. **Rook on open file bonus stacks**: rook mobility + open file bonus
   both apply (they measure different things).

---

## Interaction with other eval terms

Mobility overlaps somewhat with piece-square tables (which already reward
central knights, long-diagonal bishops, etc.). Keep bonus values small to
avoid double-counting. Tune after benchmarking.

---

## Implementation order

Do this last (after endgame phase detection) because:
- Endgame detection affects piece-square tables, which interact with mobility
- We want to see the search improvements (MVV-LVA, killers, null move, LMR)
  in the benchmark before adding more eval noise

---

## Success criteria

- All existing tests pass.
- Starting position still evaluates to 0.
- Knights visibly prefer central squares in self-play.
- Bishops avoid closed positions.
- Benchmark shows improved ELO vs Stockfish (or at minimum no regression).
