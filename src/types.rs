#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    #[allow(dead_code)]
    pub fn opposite(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    /// The rank index (0-7) where pawns of this color start.
    pub fn pawn_start_rank(self) -> u8 {
        match self { Color::White => 1, Color::Black => 6 }
    }

    /// The rank index (0-7) where pawns of this color promote.
    pub fn pawn_promotion_rank(self) -> u8 {
        match self { Color::White => 7, Color::Black => 0 }
    }

    /// The direction pawns of this color move: +1 for White (up), -1 for Black (down).
    pub fn pawn_direction(self) -> i8 {
        match self { Color::White => 1, Color::Black => -1 }
    }

    /// The back rank index (0-7) for this color — where the king starts.
    pub fn back_rank(self) -> u8 {
        match self { Color::White => 0, Color::Black => 7 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum PieceKind {
    Pawn   = 0,
    Knight = 1,
    Bishop = 2,
    Rook   = 3,
    Queen  = 4,
    King   = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

impl Piece {
    pub fn new(color: Color, kind: PieceKind) -> Piece {
        Piece { color, kind }
    }
}

/// A board square, represented as an index 0-63.
/// Layout: a1=0, b1=1, ..., h1=7, a2=8, ..., h8=63.
/// File = column (a=0 .. h=7). Rank = row (1=0 .. 8=7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Square(u8);

impl Square {
    #[allow(dead_code)]
    pub fn new(index: u8) -> Square {
        debug_assert!(index < 64, "square index {index} out of range");
        Square(index)
    }

    pub fn from_file_rank(file: u8, rank: u8) -> Square {
        debug_assert!(file < 8 && rank < 8, "file {file} or rank {rank} out of range");
        Square(rank * 8 + file)
    }

    pub fn file(self) -> u8 {
        self.0 % 8
    }

    pub fn rank(self) -> u8 {
        self.0 / 8
    }

    pub fn index(self) -> u8 {
        self.0
    }

    /// Convert to algebraic notation, e.g. square 0 -> "a1", square 63 -> "h8".
    pub fn to_algebraic(self) -> String {
        let file_char = (b'a' + self.file()) as char;
        let rank_char = (b'1' + self.rank()) as char;
        format!("{file_char}{rank_char}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_opposite() {
        assert_eq!(Color::White.opposite(), Color::Black);
        assert_eq!(Color::Black.opposite(), Color::White);
    }

    #[test]
    fn square_a1_is_index_zero() {
        let sq = Square::from_file_rank(0, 0);
        assert_eq!(sq.index(), 0);
        assert_eq!(sq.file(), 0);
        assert_eq!(sq.rank(), 0);
        assert_eq!(sq.to_algebraic(), "a1");
    }

    #[test]
    fn square_h8_is_index_63() {
        let sq = Square::from_file_rank(7, 7);
        assert_eq!(sq.index(), 63);
        assert_eq!(sq.file(), 7);
        assert_eq!(sq.rank(), 7);
        assert_eq!(sq.to_algebraic(), "h8");
    }

    #[test]
    fn square_e4_correct() {
        // e4: file=4 (e is the 5th file, index 4), rank=3 (rank 4 is index 3)
        let sq = Square::from_file_rank(4, 3);
        assert_eq!(sq.to_algebraic(), "e4");
    }

    #[test]
    fn piece_construction() {
        let p = Piece::new(Color::White, PieceKind::Knight);
        assert_eq!(p.color, Color::White);
        assert_eq!(p.kind, PieceKind::Knight);
    }
}
