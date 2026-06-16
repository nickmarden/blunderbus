use crate::position::Position;
use crate::types::{Color, Piece, PieceKind, Square};

/// The kind of move being made. Normal covers most moves; the others
/// require special handling when the move is applied to a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    Normal,
    EnPassant,
    CastleKingside,
    CastleQueenside,
    Promotion(PieceKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub kind: MoveKind,
}

impl Move {
    pub fn normal(from: Square, to: Square) -> Move {
        Move { from, to, kind: MoveKind::Normal }
    }
}

/// Generate all legal moves for the side to move.
/// Filters pseudo-legal moves by applying each and checking the king is not left in check.
pub fn generate_legal_moves(pos: &Position) -> Vec<Move> {
    let color = pos.side_to_move;
    generate_pseudo_legal_moves(pos)
        .into_iter()
        .filter(|mv| {
            let after = pos.make_move(mv);
            !after.is_in_check(color)
        })
        .collect()
}

/// Generate all pseudo-legal moves for the side to move.
/// Pseudo-legal means moves follow piece rules but may leave the king in check.
/// Legality filtering happens in generate_legal_moves (not yet implemented).
pub fn generate_pseudo_legal_moves(pos: &Position) -> Vec<Move> {
    let mut moves = Vec::new();
    let color = pos.side_to_move;

    for index in 0..64u8 {
        let sq = Square::new(index);
        if let Some(piece) = pos.board.get(sq) {
            if piece.color == color {
                match piece.kind {
                    PieceKind::Knight => gen_knight_moves(pos, sq, color, &mut moves),
                    PieceKind::King   => gen_king_moves(pos, sq, color, &mut moves),
                    PieceKind::Rook   => gen_rook_moves(pos, sq, color, &mut moves),
                    PieceKind::Bishop => gen_bishop_moves(pos, sq, color, &mut moves),
                    PieceKind::Queen  => gen_queen_moves(pos, sq, color, &mut moves),
                    PieceKind::Pawn   => gen_pawn_moves(pos, sq, color, &mut moves),
                }
            }
        }
    }

    moves
}

// --- Knight ---

const KNIGHT_OFFSETS: [(i8, i8); 8] = [
    (-2, -1), (-2, 1), (-1, -2), (-1, 2),
    ( 1, -2), ( 1, 2), ( 2, -1), ( 2, 1),
];

fn gen_knight_moves(pos: &Position, from: Square, color: Color, moves: &mut Vec<Move>) {
    let (file, rank) = (from.file() as i8, from.rank() as i8);
    for (df, dr) in KNIGHT_OFFSETS {
        let (f, r) = (file + df, rank + dr);
        if in_bounds(f, r) {
            let to = Square::from_file_rank(f as u8, r as u8);
            if !occupied_by(pos, to, color) {
                moves.push(Move::normal(from, to));
            }
        }
    }
}

// --- King ---

const KING_OFFSETS: [(i8, i8); 8] = [
    (-1, -1), (-1, 0), (-1, 1),
    ( 0, -1),           (0, 1),
    ( 1, -1), ( 1, 0), ( 1, 1),
];

fn gen_king_moves(pos: &Position, from: Square, color: Color, moves: &mut Vec<Move>) {
    let (file, rank) = (from.file() as i8, from.rank() as i8);
    for (df, dr) in KING_OFFSETS {
        let (f, r) = (file + df, rank + dr);
        if in_bounds(f, r) {
            let to = Square::from_file_rank(f as u8, r as u8);
            if !occupied_by(pos, to, color) {
                moves.push(Move::normal(from, to));
            }
        }
    }

    // Castling: squares must be empty, and the king must not start in check,
    // pass through an attacked square, or land on an attacked square.
    let back_rank = color.back_rank();
    let rights = pos.castling;
    let opp = color.opposite();

    let (can_kingside, can_queenside) = match color {
        Color::White => (rights.white_kingside, rights.white_queenside),
        Color::Black => (rights.black_kingside, rights.black_queenside),
    };

    if can_kingside {
        let f1 = Square::from_file_rank(5, back_rank);
        let g1 = Square::from_file_rank(6, back_rank);
        if pos.board.get(f1).is_none()
            && pos.board.get(g1).is_none()
            && !pos.is_square_attacked(from, opp)
            && !pos.is_square_attacked(f1, opp)
            && !pos.is_square_attacked(g1, opp)
        {
            moves.push(Move { from, to: g1, kind: MoveKind::CastleKingside });
        }
    }

    if can_queenside {
        let b1 = Square::from_file_rank(1, back_rank);
        let c1 = Square::from_file_rank(2, back_rank);
        let d1 = Square::from_file_rank(3, back_rank);
        if pos.board.get(b1).is_none()
            && pos.board.get(c1).is_none()
            && pos.board.get(d1).is_none()
            && !pos.is_square_attacked(from, opp)
            && !pos.is_square_attacked(d1, opp)
            && !pos.is_square_attacked(c1, opp)
        {
            moves.push(Move { from, to: c1, kind: MoveKind::CastleQueenside });
        }
    }
}

// --- Sliding pieces ---

fn gen_ray_moves(
    pos: &Position,
    from: Square,
    color: Color,
    directions: &[(i8, i8)],
    moves: &mut Vec<Move>,
) {
    let (file, rank) = (from.file() as i8, from.rank() as i8);
    for &(df, dr) in directions {
        let (mut f, mut r) = (file + df, rank + dr);
        while in_bounds(f, r) {
            let to = Square::from_file_rank(f as u8, r as u8);
            if let Some(piece) = pos.board.get(to) {
                if piece.color != color {
                    moves.push(Move::normal(from, to)); // capture
                }
                break; // blocked regardless
            }
            moves.push(Move::normal(from, to));
            f += df;
            r += dr;
        }
    }
}

const ROOK_DIRS:   [(i8, i8); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];
const BISHOP_DIRS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
const QUEEN_DIRS:  [(i8, i8); 8] = [
    (0, 1), (0, -1), (1, 0), (-1, 0),
    (1, 1), (1, -1), (-1, 1), (-1, -1),
];

fn gen_rook_moves(pos: &Position, from: Square, color: Color, moves: &mut Vec<Move>) {
    gen_ray_moves(pos, from, color, &ROOK_DIRS, moves);
}

fn gen_bishop_moves(pos: &Position, from: Square, color: Color, moves: &mut Vec<Move>) {
    gen_ray_moves(pos, from, color, &BISHOP_DIRS, moves);
}

fn gen_queen_moves(pos: &Position, from: Square, color: Color, moves: &mut Vec<Move>) {
    gen_ray_moves(pos, from, color, &QUEEN_DIRS, moves);
}

// --- Pawns ---

fn gen_pawn_moves(pos: &Position, from: Square, color: Color, moves: &mut Vec<Move>) {
    let file = from.file() as i8;
    let rank = from.rank() as i8;

    // White moves up (rank increases), Black moves down (rank decreases)
    let dir = color.pawn_direction();
    let start_rank = color.pawn_start_rank();
    let promo_rank = color.pawn_promotion_rank();

    // Single push
    let push_r = rank + dir;
    if in_bounds(file, push_r) {
        let push_sq = Square::from_file_rank(file as u8, push_r as u8);
        if pos.board.get(push_sq).is_none() {
            if push_sq.rank() == promo_rank {
                push_promotions(from, push_sq, moves);
            } else {
                moves.push(Move::normal(from, push_sq));

                // Double push from starting rank
                let double_r = rank + dir * 2;
                if from.rank() == start_rank && in_bounds(file, double_r) {
                    let double_sq = Square::from_file_rank(file as u8, double_r as u8);
                    if pos.board.get(double_sq).is_none() {
                        moves.push(Move::normal(from, double_sq));
                    }
                }
            }
        }
    }

    // Diagonal captures
    for df in [-1i8, 1i8] {
        let (cf, cr) = (file + df, rank + dir);
        if !in_bounds(cf, cr) {
            continue;
        }
        let cap_sq = Square::from_file_rank(cf as u8, cr as u8);

        // Normal capture
        if let Some(target) = pos.board.get(cap_sq) {
            if target.color != color {
                if cap_sq.rank() == promo_rank {
                    push_promotions(from, cap_sq, moves);
                } else {
                    moves.push(Move::normal(from, cap_sq));
                }
            }
        }

        // En passant
        if Some(cap_sq) == pos.en_passant {
            moves.push(Move { from, to: cap_sq, kind: MoveKind::EnPassant });
        }
    }
}

/// Push one move per promotion choice (queen, rook, bishop, knight).
fn push_promotions(from: Square, to: Square, moves: &mut Vec<Move>) {
    for kind in [PieceKind::Queen, PieceKind::Rook, PieceKind::Bishop, PieceKind::Knight] {
        moves.push(Move { from, to, kind: MoveKind::Promotion(kind) });
    }
}

// --- Helpers ---

fn in_bounds(file: i8, rank: i8) -> bool {
    file >= 0 && file < 8 && rank >= 0 && rank < 8
}

fn occupied_by(pos: &Position, sq: Square, color: Color) -> bool {
    matches!(pos.board.get(sq), Some(Piece { color: c, .. }) if c == color)
}

/// Count all legal positions reachable at exactly `depth` half-moves.
/// The well-known starting-position values are the gold standard for move generator correctness.
pub fn perft(pos: &Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let moves = generate_legal_moves(pos);
    if depth == 1 {
        return moves.len() as u64;
    }
    moves.iter().map(|mv| perft(&pos.make_move(mv), depth - 1)).sum()
}

/// Perft broken down by first move — essential for diagnosing incorrect counts.
#[allow(dead_code)]
pub fn perft_divide(pos: &Position, depth: u32) -> Vec<(String, u64)> {
    let mut results: Vec<(String, u64)> = generate_legal_moves(pos)
        .iter()
        .map(|mv| {
            let label = format!("{}{}", mv.from.to_algebraic(), mv.to.to_algebraic());
            let count = if depth > 1 { perft(&pos.make_move(mv), depth - 1) } else { 1 };
            (label, count)
        })
        .collect();
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starting_pos() -> Position {
        Position::starting_position()
    }

    fn count_moves(pos: &Position, sq: &str) -> usize {
        let sq = parse_sq(sq);
        generate_pseudo_legal_moves(pos)
            .into_iter()
            .filter(|m| m.from == sq)
            .count()
    }

    fn parse_sq(s: &str) -> Square {
        let b = s.as_bytes();
        Square::from_file_rank(b[0] - b'a', b[1] - b'1')
    }

    #[test]
    fn perft_depth_1() {
        assert_eq!(perft(&starting_pos(), 1), 20);
    }

    #[test]
    fn perft_depth_2() {
        assert_eq!(perft(&starting_pos(), 2), 400);
    }

    #[test]
    fn perft_depth_3() {
        assert_eq!(perft(&starting_pos(), 3), 8_902);
    }

    #[test]
    #[ignore]
    fn perft_depth_4() {
        assert_eq!(perft(&starting_pos(), 4), 197_281);
    }

    #[test]
    #[ignore]
    fn perft_depth_5() {
        assert_eq!(perft(&starting_pos(), 5), 4_865_609);
    }

    #[test]
    fn starting_position_has_20_pseudo_legal_moves() {
        // 16 pawn moves (8 pawns x 2 squares each) + 4 knight moves
        let pos = starting_pos();
        let moves = generate_pseudo_legal_moves(&pos);
        assert_eq!(moves.len(), 20);
    }

    #[test]
    fn knight_on_b1_has_two_moves_at_start() {
        assert_eq!(count_moves(&starting_pos(), "b1"), 2);
    }

    #[test]
    fn knight_in_center_has_eight_moves() {
        // Empty board, knight on e4
        let mut pos = Position::starting_position();
        pos.board.set(Square::from_file_rank(4, 3), Some(Piece::new(Color::White, PieceKind::Knight)));
        // Don't count starting position knights conflicting — use isolated test position
        let pos = Position::from_fen("8/8/8/8/4N3/8/8/8 w - - 0 1").unwrap();
        let moves = generate_pseudo_legal_moves(&pos);
        assert_eq!(moves.len(), 8);
    }

    #[test]
    fn pawn_on_e2_has_two_moves() {
        assert_eq!(count_moves(&starting_pos(), "e2"), 2);
    }

    // Castling legality: king may not castle through or out of check.

    #[test]
    fn cannot_castle_kingside_through_check() {
        // Black rook on f8 covers f1 — White's kingside transit square.
        let pos = Position::from_fen("5r2/8/8/8/8/8/8/4K2R w K - 0 1").unwrap();
        let moves = generate_legal_moves(&pos);
        assert!(!moves.iter().any(|m| m.kind == MoveKind::CastleKingside),
            "should not be able to castle kingside through f1 while it is attacked");
    }

    #[test]
    fn cannot_castle_queenside_through_check() {
        // Black rook on d8 covers d1 — White's queenside transit square.
        let pos = Position::from_fen("3r4/8/8/8/8/8/8/R3K3 w Q - 0 1").unwrap();
        let moves = generate_legal_moves(&pos);
        assert!(!moves.iter().any(|m| m.kind == MoveKind::CastleQueenside),
            "should not be able to castle queenside through d1 while it is attacked");
    }

    #[test]
    fn cannot_castle_while_in_check() {
        // Black rook on e8 gives check on e1 — king may not castle out of check.
        let pos = Position::from_fen("4r3/8/8/8/8/8/8/4K2R w K - 0 1").unwrap();
        let moves = generate_legal_moves(&pos);
        assert!(!moves.iter().any(|m| m.kind == MoveKind::CastleKingside),
            "should not be able to castle while in check");
    }
}
