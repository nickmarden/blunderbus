use crate::bitboard::{king_attacks, knight_attacks, Bitboard, RANK_3, RANK_6};
use crate::position::Position;
use crate::types::{Color, PieceKind, Square};

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
                    PieceKind::Pawn   => {} // handled below via bulk bitboard generator
                }
            }
        }
    }
    gen_pawn_moves_bb(pos, color, &mut moves);

    moves
}

// --- Knight ---

fn gen_knight_moves(pos: &Position, from: Square, color: Color, moves: &mut Vec<Move>) {
    let mut targets = knight_attacks()[from.index() as usize]
        & !pos.bbs.color_occupancy(color);
    while !targets.is_empty() {
        moves.push(Move::normal(from, targets.pop_lsb()));
    }
}

// --- King ---

fn gen_king_moves(pos: &Position, from: Square, color: Color, moves: &mut Vec<Move>) {
    let mut targets = king_attacks()[from.index() as usize]
        & !pos.bbs.color_occupancy(color);
    while !targets.is_empty() {
        moves.push(Move::normal(from, targets.pop_lsb()));
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

// Each array holds the bitboard shift functions for that piece's movement directions.
// Shift functions handle edge masking internally (east/west mask FILE_H/FILE_A before shifting).
const ROOK_SHIFTS:   [fn(Bitboard) -> Bitboard; 4] = [
    Bitboard::north, Bitboard::south, Bitboard::east, Bitboard::west,
];
const BISHOP_SHIFTS: [fn(Bitboard) -> Bitboard; 4] = [
    Bitboard::north_east, Bitboard::north_west, Bitboard::south_east, Bitboard::south_west,
];
const QUEEN_SHIFTS:  [fn(Bitboard) -> Bitboard; 8] = [
    Bitboard::north, Bitboard::south, Bitboard::east, Bitboard::west,
    Bitboard::north_east, Bitboard::north_west, Bitboard::south_east, Bitboard::south_west,
];

/// Walk each ray direction one step at a time using bitboard shifts.
/// The shift function's edge masking replaces the old in_bounds check.
fn gen_slider_moves(
    pos: &Position,
    from: Square,
    color: Color,
    shifts: &[fn(Bitboard) -> Bitboard],
    moves: &mut Vec<Move>,
) {
    let own_occ = pos.bbs.color_occupancy(color);
    let any_occ = pos.bbs.occupancy();
    let from_bb = Bitboard::from_square(from);
    for &shift in shifts {
        let mut cur = shift(from_bb);
        while !cur.is_empty() {
            if !(cur & own_occ).is_empty() { break; } // own piece: stop without moving here
            moves.push(Move::normal(from, cur.lsb()));
            if !(cur & any_occ).is_empty() { break; } // any piece: stop after capturing
            cur = shift(cur);
        }
    }
}

fn gen_rook_moves(pos: &Position, from: Square, color: Color, moves: &mut Vec<Move>) {
    gen_slider_moves(pos, from, color, &ROOK_SHIFTS, moves);
}

fn gen_bishop_moves(pos: &Position, from: Square, color: Color, moves: &mut Vec<Move>) {
    gen_slider_moves(pos, from, color, &BISHOP_SHIFTS, moves);
}

fn gen_queen_moves(pos: &Position, from: Square, color: Color, moves: &mut Vec<Move>) {
    gen_slider_moves(pos, from, color, &QUEEN_SHIFTS, moves);
}

// --- Pawns ---

/// Generate all pawn moves for `color` using bitboard shift arithmetic.
/// All single-push, double-push, diagonal capture, promotion, and en-passant
/// moves are computed in bulk rather than per-pawn.
fn gen_pawn_moves_bb(pos: &Position, color: Color, moves: &mut Vec<Move>) {
    let pawns   = pos.bbs.pieces(color, PieceKind::Pawn);
    let empty   = !pos.bbs.occupancy();
    let opp_occ = pos.bbs.color_occupancy(color.opposite());

    match color {
        Color::White => {
            // Single pushes northward; save target set for double-push seed.
            let single = pawns.north() & empty;

            let mut iter = single;
            while !iter.is_empty() {
                let to   = iter.pop_lsb();
                let from = Square::new(to.index() - 8);
                if to.rank() == 7 { push_promotions(from, to, moves); }
                else               { moves.push(Move::normal(from, to)); }
            }

            // Double push: only pawns whose rank-3 square was empty (captured by `single`).
            let mut dp = (single & Bitboard(RANK_3)).north() & empty;
            while !dp.is_empty() {
                let to   = dp.pop_lsb();
                let from = Square::new(to.index() - 16);
                moves.push(Move::normal(from, to));
            }

            // Northeast captures (file+1, rank+1 = index+9).
            let mut ne = pawns.north_east() & opp_occ;
            while !ne.is_empty() {
                let to   = ne.pop_lsb();
                let from = Square::new(to.index() - 9);
                if to.rank() == 7 { push_promotions(from, to, moves); }
                else               { moves.push(Move::normal(from, to)); }
            }

            // Northwest captures (file-1, rank+1 = index+7).
            let mut nw = pawns.north_west() & opp_occ;
            while !nw.is_empty() {
                let to   = nw.pop_lsb();
                let from = Square::new(to.index() - 7);
                if to.rank() == 7 { push_promotions(from, to, moves); }
                else               { moves.push(Move::normal(from, to)); }
            }

            // En passant.
            if let Some(ep) = pos.en_passant {
                let ep_bb = Bitboard::from_square(ep);
                let mut ne = pawns.north_east() & ep_bb;
                if !ne.is_empty() {
                    let to = ne.pop_lsb();
                    moves.push(Move { from: Square::new(to.index() - 9), to, kind: MoveKind::EnPassant });
                }
                let mut nw = pawns.north_west() & ep_bb;
                if !nw.is_empty() {
                    let to = nw.pop_lsb();
                    moves.push(Move { from: Square::new(to.index() - 7), to, kind: MoveKind::EnPassant });
                }
            }
        }

        Color::Black => {
            // Single pushes southward.
            let single = pawns.south() & empty;

            let mut iter = single;
            while !iter.is_empty() {
                let to   = iter.pop_lsb();
                let from = Square::new(to.index() + 8);
                if to.rank() == 0 { push_promotions(from, to, moves); }
                else               { moves.push(Move::normal(from, to)); }
            }

            // Double push: only pawns whose rank-6 square was empty.
            let mut dp = (single & Bitboard(RANK_6)).south() & empty;
            while !dp.is_empty() {
                let to   = dp.pop_lsb();
                let from = Square::new(to.index() + 16);
                moves.push(Move::normal(from, to));
            }

            // Southeast captures (file+1, rank-1 = index-7).
            let mut se = pawns.south_east() & opp_occ;
            while !se.is_empty() {
                let to   = se.pop_lsb();
                let from = Square::new(to.index() + 7);
                if to.rank() == 0 { push_promotions(from, to, moves); }
                else               { moves.push(Move::normal(from, to)); }
            }

            // Southwest captures (file-1, rank-1 = index-9).
            let mut sw = pawns.south_west() & opp_occ;
            while !sw.is_empty() {
                let to   = sw.pop_lsb();
                let from = Square::new(to.index() + 9);
                if to.rank() == 0 { push_promotions(from, to, moves); }
                else               { moves.push(Move::normal(from, to)); }
            }

            // En passant.
            if let Some(ep) = pos.en_passant {
                let ep_bb = Bitboard::from_square(ep);
                let mut se = pawns.south_east() & ep_bb;
                if !se.is_empty() {
                    let to = se.pop_lsb();
                    moves.push(Move { from: Square::new(to.index() + 7), to, kind: MoveKind::EnPassant });
                }
                let mut sw = pawns.south_west() & ep_bb;
                if !sw.is_empty() {
                    let to = sw.pop_lsb();
                    moves.push(Move { from: Square::new(to.index() + 9), to, kind: MoveKind::EnPassant });
                }
            }
        }
    }
}

/// Push one move per promotion choice (queen, rook, bishop, knight).
fn push_promotions(from: Square, to: Square, moves: &mut Vec<Move>) {
    for kind in [PieceKind::Queen, PieceKind::Rook, PieceKind::Bishop, PieceKind::Knight] {
        moves.push(Move { from, to, kind: MoveKind::Promotion(kind) });
    }
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
    use crate::types::Piece;

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

    // Pawn move generation via bitboard shifts.

    #[test]
    fn pawn_promotion_generates_four_moves() {
        // White pawn on e7 with nothing blocking — should generate 4 promotion moves.
        let pos = Position::from_fen("8/4P3/8/8/8/8/8/4K2k w - - 0 1").unwrap();
        let moves = generate_pseudo_legal_moves(&pos);
        let promos: Vec<_> = moves.iter().filter(|m| matches!(m.kind, MoveKind::Promotion(_))).collect();
        assert_eq!(promos.len(), 4, "e7-e8 should produce Q/R/B/N promotions");
    }

    #[test]
    fn en_passant_capture_generated() {
        // White pawn on e5, black pawn just double-pushed to d5, en-passant square is d6.
        let pos = Position::from_fen("8/8/8/3pP3/8/8/8/4K2k w - d6 0 1").unwrap();
        let moves = generate_pseudo_legal_moves(&pos);
        assert!(moves.iter().any(|m| m.kind == MoveKind::EnPassant),
            "en passant capture on d6 should be generated");
    }

    #[test]
    fn pawn_blocked_cannot_push() {
        // White pawn on e4 with a black piece on e5 — no push should be generated.
        let pos = Position::from_fen("8/8/8/4p3/4P3/8/8/4K2k w - - 0 1").unwrap();
        let sq = parse_sq("e4");
        let moves = generate_pseudo_legal_moves(&pos);
        assert!(!moves.iter().any(|m| m.from == sq), "blocked pawn on e4 should have no moves");
    }

    #[test]
    fn black_pawn_double_push_from_start() {
        // Black pawn on d7 with clear d6 and d5.
        let pos = Position::from_fen("4k3/3p4/8/8/8/8/8/4K3 b - - 0 1").unwrap();
        let sq = parse_sq("d7");
        let moves = generate_pseudo_legal_moves(&pos);
        let pawn_moves: Vec<_> = moves.iter().filter(|m| m.from == sq).collect();
        assert_eq!(pawn_moves.len(), 2, "black d7 pawn should have single and double push");
    }

    // Slider move generation via bitboard shifts.

    #[test]
    fn rook_on_empty_board_has_14_moves() {
        // Rook on e4, kings off the e-file so no ray is blocked: 7 rank + 7 file = 14.
        let pos = Position::from_fen("8/8/8/8/4R3/8/8/K6k w - - 0 1").unwrap();
        let sq = parse_sq("e4");
        let moves = generate_pseudo_legal_moves(&pos);
        let rook_moves: Vec<_> = moves.iter().filter(|m| m.from == sq).collect();
        assert_eq!(rook_moves.len(), 14);
    }

    #[test]
    fn rook_blocked_by_own_piece_stops_before_it() {
        // White rook on a1, white pawn on a4 — rook can only go a2, a3 northward.
        let pos = Position::from_fen("8/8/8/8/P7/8/8/R3K2k w - - 0 1").unwrap();
        let sq = parse_sq("a1");
        let moves = generate_pseudo_legal_moves(&pos);
        let northward: Vec<_> = moves.iter()
            .filter(|m| m.from == sq && m.to.file() == 0 && m.to.rank() > 0)
            .collect();
        assert_eq!(northward.len(), 2, "rook should reach a2 and a3 but not beyond the pawn on a4");
    }

    #[test]
    fn bishop_on_empty_board_has_13_moves() {
        // Bishop on d4 (center-ish): sum of all diagonal squares = 13.
        let pos = Position::from_fen("8/8/8/8/3B4/8/8/4K2k w - - 0 1").unwrap();
        let sq = parse_sq("d4");
        let moves = generate_pseudo_legal_moves(&pos);
        let bishop_moves: Vec<_> = moves.iter().filter(|m| m.from == sq).collect();
        assert_eq!(bishop_moves.len(), 13);
    }
}
