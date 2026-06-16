use crate::position::Position;
use crate::types::{Color, PieceKind, Square};

/// Evaluate the position in centipawns from White's perspective.
/// Positive = White is better, negative = Black is better.
pub fn evaluate(pos: &Position) -> i32 {
    let mut score = 0i32;

    for color in [Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        for kind in [PieceKind::Pawn, PieceKind::Knight, PieceKind::Bishop,
                     PieceKind::Rook, PieceKind::Queen, PieceKind::King] {
            let mut bb = pos.bbs.pieces(color, kind);
            while !bb.is_empty() {
                let sq = bb.pop_lsb();
                score += sign * (material_value(kind) + piece_square_bonus(kind, color, sq));
            }
        }
    }

    score
}

fn material_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn   =>    100,
        PieceKind::Knight =>    320,
        PieceKind::Bishop =>    330,
        PieceKind::Rook   =>    500,
        PieceKind::Queen  =>    900,
        PieceKind::King   => 20_000,
    }
}

fn piece_square_bonus(kind: PieceKind, color: Color, sq: Square) -> i32 {
    let table: &[i32; 64] = match kind {
        PieceKind::Pawn   => &PAWN_TABLE,
        PieceKind::Knight => &KNIGHT_TABLE,
        PieceKind::Bishop => &BISHOP_TABLE,
        PieceKind::Rook   => &ROOK_TABLE,
        PieceKind::Queen  => &QUEEN_TABLE,
        PieceKind::King   => &KING_TABLE,
    };
    let idx = match color {
        Color::White => (7 - sq.rank() as usize) * 8 + sq.file() as usize,
        Color::Black =>      sq.rank() as usize  * 8 + sq.file() as usize,
    };
    table[idx]
}

// Piece-square tables. Written rank 8 (top) to rank 1 (bottom), a-file to h-file.
// Values are bonuses in centipawns from that color's perspective.

#[rustfmt::skip]
const PAWN_TABLE: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
    50, 50, 50, 50, 50, 50, 50, 50,
    10, 10, 20, 30, 30, 20, 10, 10,
     5,  5, 10, 25, 25, 10,  5,  5,
     0,  0,  0, 20, 20,  0,  0,  0,
     5, -5,-10,  0,  0,-10, -5,  5,
     5, 10, 10,-20,-20, 10, 10,  5,
     0,  0,  0,  0,  0,  0,  0,  0,
];

#[rustfmt::skip]
const KNIGHT_TABLE: [i32; 64] = [
    -50,-40,-30,-30,-30,-30,-40,-50,
    -40,-20,  0,  0,  0,  0,-20,-40,
    -30,  0, 10, 15, 15, 10,  0,-30,
    -30,  5, 15, 20, 20, 15,  5,-30,
    -30,  0, 15, 20, 20, 15,  0,-30,
    -30,  5, 10, 15, 15, 10,  5,-30,
    -40,-20,  0,  5,  5,  0,-20,-40,
    -50,-40,-30,-30,-30,-30,-40,-50,
];

#[rustfmt::skip]
const BISHOP_TABLE: [i32; 64] = [
    -20,-10,-10,-10,-10,-10,-10,-20,
    -10,  0,  0,  0,  0,  0,  0,-10,
    -10,  0,  5, 10, 10,  5,  0,-10,
    -10,  5,  5, 10, 10,  5,  5,-10,
    -10,  0, 10, 10, 10, 10,  0,-10,
    -10, 10, 10, 10, 10, 10, 10,-10,
    -10,  5,  0,  0,  0,  0,  5,-10,
    -20,-10,-10,-10,-10,-10,-10,-20,
];

#[rustfmt::skip]
const ROOK_TABLE: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
     5, 10, 10, 10, 10, 10, 10,  5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
     0,  0,  0,  5,  5,  0,  0,  0,
];

#[rustfmt::skip]
const QUEEN_TABLE: [i32; 64] = [
    -20,-10,-10, -5, -5,-10,-10,-20,
    -10,  0,  0,  0,  0,  0,  0,-10,
    -10,  0,  5,  5,  5,  5,  0,-10,
     -5,  0,  5,  5,  5,  5,  0, -5,
      0,  0,  5,  5,  5,  5,  0, -5,
    -10,  5,  5,  5,  5,  5,  0,-10,
    -10,  0,  5,  0,  0,  0,  0,-10,
    -20,-10,-10, -5, -5,-10,-10,-20,
];

#[rustfmt::skip]
const KING_TABLE: [i32; 64] = [
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -20,-30,-30,-40,-40,-30,-30,-20,
    -10,-20,-20,-20,-20,-20,-20,-10,
     20, 20,  0,  0,  0,  0, 20, 20,
     20, 30, 10,  0,  0, 10, 30, 20,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_position_is_equal() {
        let pos = Position::starting_position();
        assert_eq!(evaluate(&pos), 0, "starting position should be perfectly balanced");
    }

    #[test]
    fn extra_white_pawn_scores_positive() {
        // White has an extra pawn on e4; Black's e5 pawn is gone
        let pos = Position::from_fen("rnbqkbnr/pppp1ppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1")
            .unwrap();
        assert!(evaluate(&pos) > 0, "White should be ahead with an extra pawn");
    }

    #[test]
    fn extra_black_pawn_scores_negative() {
        let pos = Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1")
            .unwrap();
        assert!(evaluate(&pos) < 0, "Black should be ahead with an extra pawn");
    }

    #[test]
    fn only_kings_is_zero() {
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(evaluate(&pos), 0, "king-only position should be equal");
    }

    #[test]
    fn queen_advantage_dominates_material() {
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4KQ2 w - - 0 1").unwrap();
        assert!(evaluate(&pos) > 800, "extra queen should give large positive score");
    }
}
