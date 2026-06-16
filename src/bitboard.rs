// Items used only in later migration phases or in tests are flagged dead_code by cargo build.
// Suppress those warnings for now — they will be used as Phases 2c/2d/3 land.
#![allow(dead_code)]

use std::sync::OnceLock;

use crate::board::Board;
use crate::types::{Color, Piece, PieceKind, Square};

// --- File and rank masks ---

pub const FILE_A: u64 = 0x0101_0101_0101_0101;
pub const FILE_B: u64 = 0x0202_0202_0202_0202;
pub const FILE_G: u64 = 0x4040_4040_4040_4040;
pub const FILE_H: u64 = 0x8080_8080_8080_8080;
pub const RANK_1: u64 = 0x0000_0000_0000_00FF;
pub const RANK_2: u64 = 0x0000_0000_0000_FF00;
pub const RANK_3: u64 = 0x0000_0000_00FF_0000;
pub const RANK_4: u64 = 0x0000_0000_FF00_0000;
pub const RANK_5: u64 = 0x0000_00FF_0000_0000;
pub const RANK_6: u64 = 0x0000_FF00_0000_0000;
pub const RANK_7: u64 = 0x00FF_0000_0000_0000;
pub const RANK_8: u64 = 0xFF00_0000_0000_0000;

// --- Bitboard newtype ---

/// A set of up to 64 squares encoded as a bitmask.
/// Bit layout: bit 0 = a1, bit 7 = h1, bit 8 = a2, ..., bit 63 = h8 (LERF order).
/// This matches the Square index layout used everywhere in blunderbus.
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

    /// Return the square of the lowest-index set bit (does not modify self).
    pub fn lsb(self) -> Square {
        Square::new(self.0.trailing_zeros() as u8)
    }

    /// Remove and return the lowest-index set bit.
    pub fn pop_lsb(&mut self) -> Square {
        let idx = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1; // clear the lowest set bit
        Square::new(idx)
    }

    // Directional shifts. `east`/`west` mask off the wrap edge before shifting.
    pub fn north(self) -> Bitboard { Bitboard(self.0 << 8) }
    pub fn south(self) -> Bitboard { Bitboard(self.0 >> 8) }
    pub fn east(self)  -> Bitboard { Bitboard((self.0 & !FILE_H) << 1) }
    pub fn west(self)  -> Bitboard { Bitboard((self.0 & !FILE_A) >> 1) }
    pub fn north_east(self) -> Bitboard { Bitboard((self.0 & !FILE_H) << 9) }
    pub fn north_west(self) -> Bitboard { Bitboard((self.0 & !FILE_A) << 7) }
    pub fn south_east(self) -> Bitboard { Bitboard((self.0 & !FILE_H) >> 7) }
    pub fn south_west(self) -> Bitboard { Bitboard((self.0 & !FILE_A) >> 9) }
}

// Operator overloads so callers can write `a | b`, `a & b`, `!a`, etc.
impl std::ops::BitOr  for Bitboard { type Output = Self; fn bitor(self,  r: Self) -> Self { Bitboard(self.0 |  r.0) } }
impl std::ops::BitAnd for Bitboard { type Output = Self; fn bitand(self, r: Self) -> Self { Bitboard(self.0 &  r.0) } }
impl std::ops::BitXor for Bitboard { type Output = Self; fn bitxor(self, r: Self) -> Self { Bitboard(self.0 ^  r.0) } }
impl std::ops::Not    for Bitboard { type Output = Self; fn not(self)              -> Self { Bitboard(!self.0)       } }
impl std::ops::BitOrAssign  for Bitboard { fn bitor_assign(&mut self,  r: Self) { self.0 |=  r.0; } }
impl std::ops::BitAndAssign for Bitboard { fn bitand_assign(&mut self, r: Self) { self.0 &=  r.0; } }
impl std::ops::BitXorAssign for Bitboard { fn bitxor_assign(&mut self, r: Self) { self.0 ^=  r.0; } }

// --- Direction shift arrays ---

/// The four orthogonal ray directions used by rooks and queens.
pub const ROOK_RAYS: [fn(Bitboard) -> Bitboard; 4] = [
    Bitboard::north, Bitboard::south, Bitboard::east, Bitboard::west,
];

/// The four diagonal ray directions used by bishops and queens.
pub const BISHOP_RAYS: [fn(Bitboard) -> Bitboard; 4] = [
    Bitboard::north_east, Bitboard::north_west, Bitboard::south_east, Bitboard::south_west,
];

// --- Precomputed attack tables ---

/// Knight attack mask for each square.
pub fn knight_attacks() -> &'static [Bitboard; 64] {
    static TABLE: OnceLock<[Bitboard; 64]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [Bitboard::EMPTY; 64];
        for sq in 0u8..64 {
            let b = Bitboard::from_square(Square::new(sq));
            t[sq as usize] =
                b.north().north().east()
              | b.north().north().west()
              | b.south().south().east()
              | b.south().south().west()
              | b.north().east().east()
              | b.north().west().west()
              | b.south().east().east()
              | b.south().west().west();
        }
        t
    })
}

/// King attack mask for each square (all 8 neighbours).
pub fn king_attacks() -> &'static [Bitboard; 64] {
    static TABLE: OnceLock<[Bitboard; 64]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [Bitboard::EMPTY; 64];
        for sq in 0u8..64 {
            let b = Bitboard::from_square(Square::new(sq));
            t[sq as usize] =
                b.north() | b.south() | b.east() | b.west()
              | b.north_east() | b.north_west()
              | b.south_east() | b.south_west();
        }
        t
    })
}

// --- BitboardSet ---

/// One Bitboard per (Color, PieceKind) pair — twelve boards total.
/// Indexed as `boards[color as usize][kind as usize]`.
#[derive(Debug, Clone, Copy)]
pub struct BitboardSet {
    pub boards: [[Bitboard; 6]; 2],
}

impl BitboardSet {
    pub fn empty() -> BitboardSet {
        BitboardSet { boards: [[Bitboard::EMPTY; 6]; 2] }
    }

    /// Build from a mailbox Board by scanning all 64 squares.
    /// Called after FEN parsing and after make_move so the two representations stay in sync.
    pub fn from_board(board: &Board) -> BitboardSet {
        let mut bbs = BitboardSet::empty();
        for idx in 0..64u8 {
            let sq = Square::new(idx);
            if let Some(piece) = board.get(sq) {
                *bbs.pieces_mut(piece.color, piece.kind) |= Bitboard::from_square(sq);
            }
        }
        bbs
    }

    pub fn pieces(&self, color: Color, kind: PieceKind) -> Bitboard {
        self.boards[color as usize][kind as usize]
    }

    pub fn pieces_mut(&mut self, color: Color, kind: PieceKind) -> &mut Bitboard {
        &mut self.boards[color as usize][kind as usize]
    }

    /// All squares occupied by `color`.
    pub fn color_occupancy(&self, color: Color) -> Bitboard {
        self.boards[color as usize].iter().copied().fold(Bitboard::EMPTY, |a, b| a | b)
    }

    /// All occupied squares (both colors).
    pub fn occupancy(&self) -> Bitboard {
        self.color_occupancy(Color::White) | self.color_occupancy(Color::Black)
    }

    /// Find what piece (if any) is on `sq`. O(12) scan; use only outside the hot path.
    pub fn piece_at(&self, sq: Square) -> Option<Piece> {
        for color in [Color::White, Color::Black] {
            for kind in [
                PieceKind::Pawn, PieceKind::Knight, PieceKind::Bishop,
                PieceKind::Rook, PieceKind::Queen,  PieceKind::King,
            ] {
                if self.pieces(color, kind).contains(sq) {
                    return Some(Piece::new(color, kind));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starting_bbs() -> BitboardSet {
        use crate::board::Board;
        let board = Board::from_fen_placement("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR").unwrap();
        BitboardSet::from_board(&board)
    }

    #[test]
    fn from_square_round_trips() {
        let sq = Square::from_file_rank(4, 3); // e4
        let bb = Bitboard::from_square(sq);
        assert!(bb.contains(sq));
        assert_eq!(bb.popcount(), 1);
    }

    #[test]
    fn pop_lsb_returns_lowest_square() {
        let mut bb = Bitboard::from_square(Square::new(0))
                   | Bitboard::from_square(Square::new(5));
        let first = bb.pop_lsb();
        assert_eq!(first.index(), 0);
        assert_eq!(bb.popcount(), 1);
    }

    #[test]
    fn starting_white_pawns_count() {
        let bbs = starting_bbs();
        assert_eq!(bbs.pieces(Color::White, PieceKind::Pawn).popcount(), 8);
    }

    #[test]
    fn starting_black_pawns_count() {
        let bbs = starting_bbs();
        assert_eq!(bbs.pieces(Color::Black, PieceKind::Pawn).popcount(), 8);
    }

    #[test]
    fn starting_white_occupancy() {
        let bbs = starting_bbs();
        assert_eq!(bbs.color_occupancy(Color::White).popcount(), 16);
    }

    #[test]
    fn starting_total_occupancy() {
        let bbs = starting_bbs();
        assert_eq!(bbs.occupancy().popcount(), 32);
    }

    #[test]
    fn piece_at_finds_white_king() {
        let bbs = starting_bbs();
        let e1 = Square::from_file_rank(4, 0);
        assert_eq!(bbs.piece_at(e1), Some(Piece::new(Color::White, PieceKind::King)));
    }

    #[test]
    fn piece_at_empty_square_returns_none() {
        let bbs = starting_bbs();
        let e4 = Square::from_file_rank(4, 3);
        assert_eq!(bbs.piece_at(e4), None);
    }

    #[test]
    fn knight_attacks_center_has_eight_targets() {
        let e4 = Square::from_file_rank(4, 3);
        assert_eq!(knight_attacks()[e4.index() as usize].popcount(), 8);
    }

    #[test]
    fn knight_attacks_corner_has_two_targets() {
        let a1 = Square::from_file_rank(0, 0);
        assert_eq!(knight_attacks()[a1.index() as usize].popcount(), 2);
    }

    #[test]
    fn king_attacks_center_has_eight_targets() {
        let e4 = Square::from_file_rank(4, 3);
        assert_eq!(king_attacks()[e4.index() as usize].popcount(), 8);
    }

    #[test]
    fn king_attacks_corner_has_three_targets() {
        let a1 = Square::from_file_rank(0, 0);
        assert_eq!(king_attacks()[a1.index() as usize].popcount(), 3);
    }

    #[test]
    fn operator_or_and_not() {
        let a = Bitboard(0b1010);
        let b = Bitboard(0b1100);
        assert_eq!((a | b).0,  0b1110);
        assert_eq!((a & b).0,  0b1000);
        assert_eq!((a ^ b).0,  0b0110);
        assert_eq!((!Bitboard::EMPTY).0, u64::MAX);
    }

    #[test]
    fn shift_east_does_not_wrap() {
        // A piece on the h-file shifted east should disappear, not wrap to a-file.
        let h1 = Bitboard::from_square(Square::from_file_rank(7, 0));
        assert!(h1.east().is_empty());
    }

    #[test]
    fn shift_west_does_not_wrap() {
        let a1 = Bitboard::from_square(Square::from_file_rank(0, 0));
        assert!(a1.west().is_empty());
    }
}
