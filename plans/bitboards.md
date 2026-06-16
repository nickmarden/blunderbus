# Bitboard Migration Plan

## Summary

This document describes a plan to convert blunderbus from its current mailbox board
representation to a bitboard representation. It covers what bitboards are, why they
are faster, how the new data structures should look, and how to migrate the codebase
incrementally without ever breaking the working engine.

The migration has three phases:

- **Phase 1** — Add the `Bitboard` and `BitboardSet` types alongside the mailbox `Board`.
  No behavior changes; just infrastructure plus conversion utilities.
- **Phase 2** — Replace move generation one piece type at a time, validating each step
  with perft.
- **Phase 3** — Rewrite `is_square_attacked`, `evaluate`, and `compute_hash` to use
  bitboards natively, then retire the mailbox board.

Total estimated size: medium-to-large refactor. Phase 1 is about 200 lines of new code
with no deletions. Phase 2 is the largest piece. Phase 3 finishes the cleanup. Each phase
is independently testable and committable.

---

## Background: What Are Bitboards and Why Are They Faster?

### The core idea

A chessboard has 64 squares. A `u64` has 64 bits. The mapping is direct: bit N is square N.
If bit N is 1, something interesting is on that square; if it is 0, it is not.

You keep one `u64` per piece type per color. So instead of one array of `Option<Piece>`:

```
// mailbox (current)
squares: [Option<Piece>; 64]    // 64 bytes, one slot per square
```

you keep twelve integers:

```
// bitboards
white_pawns:   u64    // bit set = a white pawn is on that square
white_knights: u64
white_bishops: u64
white_rooks:   u64
white_queens:  u64
white_kings:   u64
black_pawns:   u64
// ...
```

### C++ analogy

Think of it like a `std::bitset<64>` per piece type, or a bitmask the way you would use
`uint64_t flags` in low-level C++. The difference is that chess engines exploit the fact
that bitwise operations on a `u64` operate on all 64 bits simultaneously, which turns
"find all squares a rook can reach" from a loop with up to 7 iterations per direction into
a handful of shift-and-mask operations.

### Why faster: three concrete reasons

**1. Population count is a single instruction.**
"How many white pawns does White have?" With mailbox you scan all 64 squares. With
bitboards: `white_pawns.count_ones()`. That is a single CPU instruction (`POPCNT`).
The Rust method is `u64::count_ones()`.

**2. Finding all pieces of a type is a bit-scan loop, not a full board scan.**
With mailbox, to find every white knight you loop all 64 squares. With bitboards you
loop only the set bits of `white_knights`. If there are 2 knights you do 2 iterations
instead of 64. The loop idiom in Rust (more on this in the Rust Notes section):

```rust
let mut bb = white_knights;
while bb != 0 {
    let sq = bb.trailing_zeros() as u8;  // index of lowest set bit
    // ... process square sq ...
    bb &= bb - 1;  // clear the lowest set bit
}
```

**3. Attack generation for sliders becomes branchless bit arithmetic.**
The slow part of the current engine is `gen_ray_moves` and `is_square_attacked`. Both
walk rays one square at a time, stopping when they hit a piece. With bitboards, the
classical approach uses precomputed "fill" operations that propagate a mask along a
ray until it hits the occupancy board. Magic bitboards go further: a perfect hash of
the occupancy pattern for a given square yields the full attack set in two array
lookups. The difference between these approaches is discussed in the Magic Bitboards
section below.

---

## Proposed Data Structures

### `Bitboard` newtype

```rust
// src/bitboard.rs  (new file)

/// A set of up to 64 squares, one bit per square.
/// Bit layout matches Square: bit 0 = a1, bit 7 = h1, bit 8 = a2, ..., bit 63 = h8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bitboard(pub u64);

impl Bitboard {
    pub const EMPTY: Bitboard = Bitboard(0);
    pub const FULL:  Bitboard = Bitboard(u64::MAX);

    pub fn from_square(sq: Square) -> Bitboard {
        Bitboard(1u64 << sq.index())
    }

    pub fn contains(self, sq: Square) -> bool {
        self.0 & (1u64 << sq.index()) != 0
    }

    pub fn is_empty(self) -> bool { self.0 == 0 }

    pub fn popcount(self) -> u32 { self.0.count_ones() }

    /// Return and remove the lowest set bit (LSB = lowest-index square).
    /// Panics if empty.
    pub fn pop_lsb(&mut self) -> Square {
        let idx = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1;
        Square::new(idx)
    }

    // Shift helpers for pawn move generation
    pub fn north(self) -> Bitboard { Bitboard(self.0 << 8) }
    pub fn south(self) -> Bitboard { Bitboard(self.0 >> 8) }
    pub fn east(self)  -> Bitboard { Bitboard((self.0 & !FILE_H) << 1) }
    pub fn west(self)  -> Bitboard { Bitboard((self.0 & !FILE_A) >> 1) }
}

// Standard operator overloads so you can write `a | b`, `a & b`, `!a`, etc.
impl std::ops::BitOr  for Bitboard { type Output = Self; fn bitor(self, r: Self)  -> Self { Bitboard(self.0 | r.0) } }
impl std::ops::BitAnd for Bitboard { type Output = Self; fn bitand(self, r: Self) -> Self { Bitboard(self.0 & r.0) } }
impl std::ops::BitXor for Bitboard { type Output = Self; fn bitxor(self, r: Self) -> Self { Bitboard(self.0 ^ r.0) } }
impl std::ops::Not    for Bitboard { type Output = Self; fn not(self)             -> Self { Bitboard(!self.0) } }
impl std::ops::BitOrAssign  for Bitboard { fn bitor_assign(&mut self, r: Self)  { self.0 |= r.0; } }
impl std::ops::BitAndAssign for Bitboard { fn bitand_assign(&mut self, r: Self) { self.0 &= r.0; } }

// File and rank masks (useful for move generation)
pub const FILE_A: u64 = 0x0101_0101_0101_0101;
pub const FILE_H: u64 = 0x8080_8080_8080_8080;
pub const RANK_1: u64 = 0x0000_0000_0000_00FF;
pub const RANK_8: u64 = 0xFF00_0000_0000_0000;
pub const RANK_2: u64 = 0x0000_0000_0000_FF00;
pub const RANK_7: u64 = 0x00FF_0000_0000_0000;
```

Why a newtype instead of a raw `u64`? The newtype prevents accidentally passing a
raw integer where a `Bitboard` is expected, and lets you implement methods and operator
overloads. In C++ terms it is like wrapping `uint64_t` in a struct with `operator|`
etc. defined.

### `BitboardSet` — all 12 boards together

```rust
/// Holds one bitboard per (color, piece kind) pair.
/// Index as boards[color_index][piece_index].
///
/// color_index: Color::White=0, Color::Black=1
/// piece_index: Pawn=0, Knight=1, Bishop=2, Rook=3, Queen=4, King=5
#[derive(Debug, Clone, Copy)]
pub struct BitboardSet {
    pub boards: [[Bitboard; 6]; 2],
}

impl BitboardSet {
    pub fn empty() -> BitboardSet {
        BitboardSet { boards: [[Bitboard::EMPTY; 6]; 2] }
    }

    pub fn pieces(&self, color: Color, kind: PieceKind) -> Bitboard {
        self.boards[color as usize][kind as usize]
    }

    pub fn pieces_mut(&mut self, color: Color, kind: PieceKind) -> &mut Bitboard {
        &mut self.boards[color as usize][kind as usize]
    }

    /// All squares occupied by a given color.
    pub fn color_occupancy(&self, color: Color) -> Bitboard {
        self.boards[color as usize].iter().copied().fold(Bitboard::EMPTY, |a, b| a | b)
    }

    /// All occupied squares (both colors).
    pub fn occupancy(&self) -> Bitboard {
        self.color_occupancy(Color::White) | self.color_occupancy(Color::Black)
    }
}
```

For the indexing to work cleanly, `Color` and `PieceKind` need to implement `as usize`.
The cleanest Rust way is to add `#[repr(usize)]` to the enums, or add explicit cast
methods. Adding `repr` is simplest and has no runtime cost.

### The transition `Board` strategy

During the migration keep both representations in `Position`:

```rust
pub struct Position {
    pub board:    Board,         // mailbox — kept alive during transition
    pub bbs:      BitboardSet,   // new — populated alongside board
    // ... rest unchanged
}
```

You add a `Position::sync_bbs()` method that rebuilds `bbs` from `board` (used during
`from_fen` and `make_move`). This is not the final design — it is scaffolding that
lets you run both in parallel while you validate the new generators. Once everything
is confirmed correct, you remove `board` and flip `sync_bbs()` to become
`sync_mailbox()` (for display and PGN only), then eventually remove that too.

---

## Migration Phases

### Phase 1: Infrastructure (no behavior changes)

Estimated size: ~200 lines of new code, 0 deletions.

Steps:

1. Add `src/bitboard.rs` with `Bitboard`, `BitboardSet`, the file/rank mask constants,
   the shift helpers, and the operator impls above.
2. Add `#[repr(usize)]` to `Color` and `PieceKind` in `src/types.rs` so they can be
   used as array indices without explicit match arms.
3. Add `bbs: BitboardSet` to `Position` in `src/position.rs`.
4. In `Position::from_fen`, after parsing the mailbox board, call a new
   `BitboardSet::from_board(&board)` constructor.
5. In `Position::make_move`, after the existing board mutations, call
   `pos.bbs = BitboardSet::from_board(&pos.board)` at the end (before `compute_hash`).
   This is slower than incremental updates but is correct and keeps the phases
   independent.
6. Add a `#[test]` that builds the starting position and asserts
   `bbs.pieces(White, Pawn).popcount() == 8` and similar sanity checks.

**Gate:** all 46 existing tests still pass, plus the new sanity tests.

---

### Phase 2: Move generation (piece by piece)

This is the largest phase. For each piece type:

1. Write a new bitboard-based generator function.
2. Add a `#[cfg(test)]` test that compares its output against the old mailbox generator
   for a set of positions (including the starting position and a few tricky FENs).
3. Replace the old generator call with the new one.
4. Run perft at depth 3 (8902 nodes) — if the count is wrong, `perft_divide` will
   immediately narrow the bug to a first move.

Work this order (easiest to hardest):

#### 2a. Knights

Knights have no ray attacks, no occupancy dependence. The pattern is a precomputed
lookup table of 64 attack masks (one per square):

```rust
// Computed at startup (or const-evaluated)
static KNIGHT_ATTACKS: [Bitboard; 64] = ...;
```

Generation becomes:
```rust
let mut knights = bbs.pieces(color, Knight);
while !knights.is_empty() {
    let from = knights.pop_lsb();
    let attacks = KNIGHT_ATTACKS[from.index() as usize];
    let targets = attacks & !bbs.color_occupancy(color); // can't capture own pieces
    // ... extract target squares and push moves
}
```

#### 2b. King (non-castling)

Same pattern as knights: a precomputed 64-entry lookup table.

Castling still requires separate logic (checking empty squares between king and rook),
which you can keep exactly as it is for now — you are replacing the loop, not the
castling check.

#### 2c. Pawns

Pawn generation is pure shift arithmetic with no loop over squares — you operate on
the entire bitboard at once:

```rust
// White single push: shift pawns north, mask off occupied squares
let single_push = (bbs.pieces(White, Pawn).north()) & !occupancy;

// White double push: push again from rank 3 only
let double_push = (single_push & Bitboard(RANK_3)).north() & !occupancy;

// White captures: shift NE and NW, intersect with black pieces
let black_occ = bbs.color_occupancy(Black);
let cap_east = (bbs.pieces(White, Pawn).north().east()) & black_occ;
let cap_west = (bbs.pieces(White, Pawn).north().west()) & black_occ;
```

Promotions are detected by intersecting results with `RANK_8` (for White) before
extracting moves. En passant adds the en-passant square to `black_occ` for the capture
mask, then the `EnPassant` move kind is emitted for that specific target square.

#### 2d. Rooks, Bishops, Queen (sliders)

This is the hard part and the main reason bitboards exist. See the Attack Generation
Design section below for the full discussion. The recommended starting point for a
learning project is **classical fill / Kogge-Stone**. You can always swap in magic
bitboards later without changing the move representation.

---

### Phase 3: Cleanup and full bitboard eval

Estimated size: moderate cleanup, net deletion of ~150 lines.

1. **`is_square_attacked` in `position.rs`:** Rewrite using the precomputed attack
   tables and the same slider attack logic used in Phase 2. This is cleaner and faster
   than the current ray walk.

2. **`evaluate` in `eval.rs`:** The piece-square table loop currently visits all 64
   squares. With bitboards it iterates only occupied squares:

   ```rust
   let mut pawns = bbs.pieces(White, Pawn);
   while !pawns.is_empty() {
       let sq = pawns.pop_lsb();
       score += material_value(Pawn) + piece_square_bonus(Pawn, White, sq);
   }
   // ... repeat for each piece type
   ```

   This is measurably faster when the board is sparse (late endgame).

3. **`compute_hash` in `position.rs`:** Currently also walks all 64 squares. Swap to
   iterating over the bitboards. The hash values are unchanged; only the iteration
   changes.

4. **Remove `board: Board` from `Position`:** Only two places still need to look up
   "what piece is on square X?":
   - `Board::Display` (pretty printing and FEN serialization)
   - `pgn.rs` / `cli.rs` (piece lookup for SAN and display)

   Replace those with a `BitboardSet::piece_at(sq) -> Option<Piece>` method that
   scans all 12 bitboards. It is O(12) instead of O(1) but is called only for display
   and SAN — never in the hot path — so performance does not matter.

5. **Delete `src/board.rs`** once nothing imports it.

---

## Attack Generation Design

### Knights and Kings: precomputed tables

These are always the same regardless of what else is on the board. Compute once at
startup (or as a `const fn`). The computation itself is shift-and-mask:

```rust
fn compute_knight_attacks(sq: u8) -> Bitboard {
    let b = Bitboard::from_square(Square::new(sq));
    // All eight L-shaped directions, masking off wraparound
    let no_a = b & !Bitboard(FILE_A);
    let no_h = b & !Bitboard(FILE_H);
    let no_ab = no_a & !(Bitboard(FILE_A).east()); // two files from left edge
    let no_gh = no_h & !(Bitboard(FILE_H).west()); // two files from right edge
    ( (no_ab).north().north().east()
    | (no_ab).south().south().east()
    | (no_gh).north().north().west()
    | (no_gh).south().south().west()
    | (no_a).north().east().east()     // wait — refine per actual direction
    | ... )
}
```

In practice it is easiest to derive knight attack masks from the fixed offset table you
already have (`KNIGHT_OFFSETS` in `movegen.rs`) during a startup initialization pass
rather than trying to express it as shift arithmetic.

### Sliders: classical fill (Kogge-Stone)

The Kogge-Stone "fill" approach computes how far a piece can slide before hitting the
occupancy board. The key insight: you fill in a direction until you hit something.

```rust
/// All squares a rook on `from` attacks, given the occupancy board.
fn rook_attacks(from: Square, occupancy: Bitboard) -> Bitboard {
    ray_attacks_north(from, occupancy)
  | ray_attacks_south(from, occupancy)
  | ray_attacks_east(from, occupancy)
  | ray_attacks_west(from, occupancy)
}

/// Squares attacked in the north direction.
fn ray_attacks_north(from: Square, occ: Bitboard) -> Bitboard {
    // Start with the rook's square; fill north until we hit occupancy.
    let mut attacks = Bitboard::from_square(from);
    let mut fill = attacks;
    // Kogge-Stone: shift and OR until the fill stops changing.
    // Because we want squares the rook CAN reach (including the blocker), we
    // include the first occupied square but not beyond.
    for _ in 0..7 {
        fill = (fill | (fill.north() & !occ));
        // This version stops at the blocker but doesn't include it;
        // include one more step to get the capture square:
    }
    // The actual implementation is typically expressed as a o^(o-2r) trick
    // or as the "o minus 2r" subtraction trick for cardinal directions.
    // See below.
    attacks
}
```

The cleanest O(1) formula for a ray in one direction (no loop needed) is the
**o^(o-2r) hyperbola quintessence** trick. For the north ray from square `r` with
occupancy bitboard `o`:

```
attacks = o ^ (o - 2*r)
```

(using the Chebyshev distance "o minus 2r" formula). This works for a single ray;
you combine all four rook rays and all four bishop rays.

For a learning project, the looped fill is fine to start — it is clear and debuggable.
Switch to the subtraction trick once it is working.

### Magic bitboards

Magic bitboards are a different approach to slider attack generation. The idea:

For each square, there is a set of "relevant" occupancy squares (the squares along the
ray that could block the slider, excluding the edge squares). For a rook on e4, for
example, there are 10 relevant squares (4 along the file, 6 along the rank, endpoints
excluded).

You compute a sparse index into a precomputed attack table by:

```
index = (occupancy & relevant_mask) * MAGIC_NUMBER >> (64 - relevant_bits)
attacks = ATTACK_TABLE[square][index]
```

The "magic number" is chosen so that the multiplication + shift perfectly maps every
distinct relevant-occupancy pattern to a distinct index with no collisions. Finding
magic numbers is done by brute-force search offline; well-known magic tables are
published and can just be copied in.

**Should you use magic bitboards?** For blunderbus at this stage, the answer is
probably no. Reasons:

- The classical fill approach is easier to understand and debug.
- Magic bitboards give perhaps 2-4x speedup over classical fill for sliders, but the
  whole point of this project is understanding, not maximizing speed.
- You can swap magic bitboards in later without changing anything except the slider
  attack functions (the move representation and the rest of the engine are unaffected).
- Implementing magic number search correctly is a non-trivial exercise on its own.

The recommendation is: implement classical fill for Phase 2, note magic bitboards as a
future optimization in the TODO list, and revisit when the engine is otherwise
complete.

---

## Rust Notes

### Bitwise operations

Rust's bitwise operators (`|`, `&`, `^`, `!`, `<<`, `>>`) work exactly like C++. The
`Bitboard` operator impls proposed above let you write `a | b` instead of `Bitboard(a.0
| b.0)`. There is no implicit conversion from integer to `Bitboard` — you must
construct explicitly, which is the safety guarantee you want.

### Useful `u64` methods

| Task | Rust | C++ equivalent |
|---|---|---|
| Population count | `x.count_ones()` | `__builtin_popcountll(x)` |
| Index of LSB (lowest set bit) | `x.trailing_zeros()` | `__builtin_ctzll(x)` |
| Index of MSB | `x.leading_zeros()` gives count from top; `63 - x.leading_zeros()` | `63 - __builtin_clzll(x)` |
| Clear LSB | `x & (x - 1)` | same |
| Isolate LSB | `x & x.wrapping_neg()` or `x & (-(x as i64) as u64)` | `x & -x` |
| Is single bit set | `x.is_power_of_two()` | `(x & (x-1)) == 0` |

### Iterating set bits

The canonical loop pattern in blunderbus will be through `Bitboard::pop_lsb()`:

```rust
let mut bb = some_bitboard;
while !bb.is_empty() {
    let sq: Square = bb.pop_lsb();
    // sq is the next occupied square; do something with it
}
```

`pop_lsb` modifies the bitboard in place (it clears the bit it returns). This is why
it takes `&mut self`. In Rust, `&mut self` means "I need exclusive mutable access to
this value" — the same contract as a non-const reference in C++. The caller owns `bb`
(it is a local `let mut`), so borrowing it mutably is fine.

### `#[repr(usize)]` on enums

Adding `#[repr(usize)]` to `Color` and `PieceKind` tells the compiler to use `usize`
as the discriminant type, which enables `color as usize` and `kind as usize` casts:

```rust
#[repr(usize)]
pub enum Color { White = 0, Black = 1 }

#[repr(usize)]
pub enum PieceKind { Pawn = 0, Knight = 1, Bishop = 2, Rook = 3, Queen = 4, King = 5 }
```

In C++ you would use an `enum class` with an explicit underlying type and a cast.

### `const` vs. `static` for lookup tables

The precomputed attack tables are large arrays (up to `[Bitboard; 64]`). Prefer
`static` over `const` for large arrays: `const` inlines the value at every use site,
while `static` gives you a single memory address. For a lookup table referenced
repeatedly in a hot loop, `static` is correct.

If the table can be computed at compile time using `const fn` arithmetic, mark it
`const` during computation then assign to a `static`. If the computation is too complex
for `const fn` (e.g., the Kogge-Stone fill), initialize it in a `std::sync::OnceLock`
the same way `src/zobrist.rs` already does for the Zobrist table.

### Wrapping arithmetic

When implementing the subtraction trick for ray attacks (`o - 2*r`), use
`wrapping_sub` and `wrapping_mul` to avoid debug-mode panics on overflow:

```rust
let attacks = occ ^ occ.wrapping_sub(from_bb.wrapping_mul(2));
```

In release mode Rust integers wrap by default on overflow in arithmetic; in debug mode
they panic. The Zobrist code already uses `wrapping_mul` for this reason.

---

## Open Questions

These should be resolved before starting Phase 2.

**1. Incremental hash updates vs. full recompute.**
`Position::compute_hash()` currently does a full scan of all 64 squares after every
`make_move`. With bitboards, incremental updates (XOR in the moved piece's new
position, XOR out its old position) would be faster. However, the full-recompute
approach was chosen deliberately for correctness and simplicity. Recommendation: keep
full recompute for now; it is already fast enough at the depths blunderbus searches.
Revisit if profiling shows it is a bottleneck.

**2. `Board` for display: keep or rebuild?**
Phase 3 proposes a `BitboardSet::piece_at(sq)` method as the replacement for mailbox
lookup in display and SAN code. The alternative is to keep a small "display board"
that is rebuilt from bitboards only when the position is rendered. Either approach is
fine; `piece_at` is simpler.

**3. Castling transit-square check.**
The current engine has a known bug: castling through check is not detected. This is
unrelated to the bitboard migration, but Phase 2 (king move generation) is the natural
time to fix it. When rewriting `gen_king_moves`, add the `is_square_attacked` check for
f1/g1 (kingside) and d1/c1 (queenside) before emitting the castling move. At that
point `is_square_attacked` still uses the old mailbox code; the fix is valid regardless.

**4. Bitboard layout: LSB = a1 or LSB = a8?**
This plan assumes LSB = a1 (bit 0 = square 0 = a1), which matches the existing
`Square` layout in `src/types.rs`. This is called "LERF" (little-endian rank-file).
Most published magic tables and attack generation code also uses this layout. Do not
change it.

**5. Magic bitboards: defer or implement now?**
Recommendation above is to defer. Flag this as a future TODO in CLAUDE.md once Phase 3
is complete.

---

## Effort Estimates

| Phase | New code | Changed code | Deleted code | Perft gate |
|---|---|---|---|---|
| 1: Infrastructure | ~200 lines | ~30 lines (Position, types) | 0 | all 46 existing tests |
| 2a: Knights | ~50 lines | ~10 lines | ~15 lines | perft depth 3 |
| 2b: King | ~60 lines | ~10 lines | ~15 lines | perft depth 3 |
| 2c: Pawns | ~80 lines | ~10 lines | ~50 lines | perft depth 3 |
| 2d: Sliders | ~120 lines | ~15 lines | ~50 lines | perft depth 4+ |
| 3: Cleanup | ~40 lines | ~60 lines | ~150 lines | all tests |

The slider step (2d) carries the most risk and is where most debugging time will go.
Start it with perft at depth 2 (400 nodes) — any slider bug will be visible immediately
at depth 2 and `perft_divide` will isolate it to a specific first move.
