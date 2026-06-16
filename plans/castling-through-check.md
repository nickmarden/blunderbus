# Fix: Castling Through Check

## Summary

The pseudo-legal move generator (`gen_king_moves` in `src/movegen.rs`) emits castling moves
without verifying that the king's transit squares are safe. The legal-move filter in
`generate_legal_moves` only applies `make_move` + `is_in_check`, which catches the king
*landing* in check but not passing *through* an attacked square.

Chess rules (FIDE article 3.8.2) forbid castling when:

1. The king is currently in check.
2. Any square the king traverses is attacked by an enemy piece.
3. The king's destination square is attacked (already caught by the existing filter).

`is_square_attacked(sq, by_color) -> bool` already exists in `src/position.rs` and is exactly
what is needed. No new infrastructure is required.

---

## Squares to Check

The king starts on e1/e8 (file 4). It passes through one transit square before landing.

| Side to move | Direction   | Transit square | Landing square |
|--------------|-------------|----------------|----------------|
| White        | Kingside    | f1 (file 5)    | g1 (file 6)    |
| White        | Queenside   | d1 (file 3)    | c1 (file 2)    |
| Black        | Kingside    | f8 (file 5)    | g8 (file 6)    |
| Black        | Queenside   | d8 (file 3)    | c8 (file 2)    |

Note: b1/b8 (file 1) must be empty for queenside castling (already checked for piece
occupancy), but the king never traverses it, so it does not need an attack check.

The king's *starting* square (e1/e8, the `from` square already in scope) must also not be
attacked — i.e. the king must not currently be in check. That check uses the same
`is_square_attacked` call on the `from` square itself.

In summary, three squares must be safe for each direction:

- **Kingside**: `from` (e-file), f-file, g-file
- **Queenside**: `from` (e-file), d-file, c-file

---

## Code Changes

All changes are in `src/movegen.rs`, inside `gen_king_moves`. The function already receives
`pos: &Position` and `color: Color`, so `pos.is_square_attacked` is directly callable.

The opponent's color is `color.opposite()`, which is already defined on `Color`.

### Current code (lines 116–131)

```rust
if can_kingside {
    let f1 = Square::from_file_rank(5, back_rank);
    let g1 = Square::from_file_rank(6, back_rank);
    if pos.board.get(f1).is_none() && pos.board.get(g1).is_none() {
        moves.push(Move { from, to: g1, kind: MoveKind::CastleKingside });
    }
}

if can_queenside {
    let b1 = Square::from_file_rank(1, back_rank);
    let c1 = Square::from_file_rank(2, back_rank);
    let d1 = Square::from_file_rank(3, back_rank);
    if pos.board.get(b1).is_none() && pos.board.get(c1).is_none() && pos.board.get(d1).is_none() {
        moves.push(Move { from, to: c1, kind: MoveKind::CastleQueenside });
    }
}
```

### Replacement code

```rust
let opp = color.opposite();

// King must not currently be in check (from square must be safe).
let king_safe = !pos.is_square_attacked(from, opp);

if can_kingside {
    let f_sq = Square::from_file_rank(5, back_rank); // transit
    let g_sq = Square::from_file_rank(6, back_rank); // landing
    if king_safe
        && pos.board.get(f_sq).is_none()
        && pos.board.get(g_sq).is_none()
        && !pos.is_square_attacked(f_sq, opp)
        && !pos.is_square_attacked(g_sq, opp)
    {
        moves.push(Move { from, to: g_sq, kind: MoveKind::CastleKingside });
    }
}

if can_queenside {
    let b_sq = Square::from_file_rank(1, back_rank); // must be empty, not traversed
    let c_sq = Square::from_file_rank(2, back_rank); // landing
    let d_sq = Square::from_file_rank(3, back_rank); // transit
    if king_safe
        && pos.board.get(b_sq).is_none()
        && pos.board.get(c_sq).is_none()
        && pos.board.get(d_sq).is_none()
        && !pos.is_square_attacked(c_sq, opp)
        && !pos.is_square_attacked(d_sq, opp)
    {
        moves.push(Move { from, to: c_sq, kind: MoveKind::CastleQueenside });
    }
}
```

The `king_safe` guard is shared between both directions: if the king is in check, neither
castling move is emitted, which avoids two redundant `is_square_attacked` calls.

The `g_sq`/`c_sq` attack checks are now redundant with the existing legal-move filter (which
already rejects a move that leaves the king in check on the landing square), but including
them here is cheap and makes the guard self-contained and readable. They can be omitted for a
minimal change if preferred.

### Estimated change size

Approximately 10 lines changed / added within the existing `gen_king_moves` body. No new
functions, no new imports, no changes outside `movegen.rs`.

---

## Edge Cases

**King in check.** The `king_safe` flag gates both directions. A king in check cannot castle
even if the path and destination are clear. This is correct per FIDE rules and not currently
enforced anywhere.

**Rook attacked, not king path.** The rules do not prohibit castling when the *rook* passes
through an attacked square (only the king's path matters). The replacement code does not check
rook transit squares, which is correct.

**Queenside b-file square.** b1/b8 must be empty (rook passes through it) but is never
occupied by the king, so no attack check is needed there. The existing emptiness check is
retained unchanged.

**Both castling directions blocked by check.** When `king_safe` is false, both `if` blocks
short-circuit immediately without calling `is_square_attacked` on the path squares. This is
a minor efficiency win over checking them independently.

---

## Testing

### Perft regression

The existing perft tests at depths 1–3 pass today with the buggy code because the standard
starting position has no early castling-through-check scenarios at those depths. They will
continue to pass after the fix.

The known correct perft values at depth 4 (197,281) and depth 5 (4,865,609) are more likely
to catch castling errors because more positions with castling rights are reachable. These tests
are currently marked `#[ignore]` for speed but should be run once to confirm no regression:

```bash
cargo test -- --ignored
```

### Dedicated unit tests to add

Add to the `#[cfg(test)]` block in `src/movegen.rs`:

**1. Cannot castle kingside through an attacked square.**
Set up White with king on e1, rook on h1, castling rights `K`, but an enemy rook on f8
(attacks f1). Confirm no `CastleKingside` move is generated.

```rust
#[test]
fn cannot_castle_kingside_through_check() {
    // White: Ke1, Rh1 vs Black: Re8 (attacks f1 via... actually use Rf8 attacking f1)
    // FEN: 5r2/8/8/8/8/8/8/4K2R w K - 0 1
    // Black rook on f8 attacks f1 — White cannot castle kingside.
    let pos = Position::from_fen("5r2/8/8/8/8/8/8/4K2R w K - 0 1").unwrap();
    let moves = generate_legal_moves(&pos);
    assert!(!moves.iter().any(|m| m.kind == MoveKind::CastleKingside));
}
```

**2. Cannot castle queenside through an attacked square.**

```rust
#[test]
fn cannot_castle_queenside_through_check() {
    // Black rook on d8 attacks d1 — White cannot castle queenside.
    // FEN: 3r4/8/8/8/8/8/8/R3K3 w Q - 0 1
    let pos = Position::from_fen("3r4/8/8/8/8/8/8/R3K3 w Q - 0 1").unwrap();
    let moves = generate_legal_moves(&pos);
    assert!(!moves.iter().any(|m| m.kind == MoveKind::CastleQueenside));
}
```

**3. Cannot castle while in check.**

```rust
#[test]
fn cannot_castle_while_in_check() {
    // Black rook on e8 gives check on e1 — White cannot castle either direction.
    // FEN: 4r3/8/8/8/8/8/8/R3K2R w KQ - 0 1
    let pos = Position::from_fen("4r3/8/8/8/8/8/8/R3K2R w KQ - 0 1").unwrap();
    let moves = generate_legal_moves(&pos);
    assert!(!moves.iter().any(|m| matches!(m.kind, MoveKind::CastleKingside | MoveKind::CastleQueenside)));
}
```

**4. Can still castle when path is clear and safe (regression guard).**

```rust
#[test]
fn can_castle_when_path_is_safe() {
    // Standard castling setup with no attackers.
    // FEN: 4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1
    let pos = Position::from_fen("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1").unwrap();
    let moves = generate_legal_moves(&pos);
    assert!(moves.iter().any(|m| m.kind == MoveKind::CastleKingside));
    assert!(moves.iter().any(|m| m.kind == MoveKind::CastleQueenside));
}
```

### Existing tests

No existing tests exercise the buggy behavior and none need to be changed. The perft counts
at depths 1–3 are unaffected because castling-through-check positions are not reachable within
three half-moves from the starting position.
