use crate::bitboard::{file_mask, front_fill};
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
        score += sign * king_safety_penalty(pos, color);
        score += sign * passed_pawn_bonus(pos, color);
    }

    score
}

/// Bonus (always >= 0) for passed pawns belonging to `color`.
///
/// A pawn is passed when no enemy pawn is on the same file or either adjacent file,
/// strictly ahead of it.  The bonus scales with how far advanced the passer is.
fn passed_pawn_bonus(pos: &Position, color: Color) -> i32 {
    let our_pawns   = pos.bbs.pieces(color, PieceKind::Pawn);
    let their_pawns = pos.bbs.pieces(color.opposite(), PieceKind::Pawn);

    // Shift enemy pawns one step in their own direction first so that an enemy pawn
    // on the SAME rank as ours (adjacent file) does not falsely block it.
    // Then fill in the enemy's direction to cover all squares they shadow toward our side.
    let shifted = match color {
        Color::White => their_pawns.south(), // Black pawns shadow squares southward
        Color::Black => their_pawns.north(), // White pawns shadow squares northward
    };
    let span      = front_fill(shifted, color.opposite());
    let wide_span = span | span.east() | span.west();

    let mut passers = our_pawns & !wide_span;
    let mut bonus   = 0i32;
    while !passers.is_empty() {
        let sq       = passers.pop_lsb();
        let rank_idx = match color {
            Color::White => sq.rank() as usize,
            Color::Black => 7 - sq.rank() as usize,
        };
        bonus += PASSED_PAWN_BONUS[rank_idx];
    }
    bonus
}

// Indexed by rank from the pawn's own perspective (0 = home rank, 7 = promotion rank).
// Ranks 0 and 7 are impossible for a pawn; the rest scale steeply as the passer advances.
const PASSED_PAWN_BONUS: [i32; 8] = [0, 0, 10, 20, 35, 55, 80, 0];

/// King safety penalty for `color` (always <= 0).
///
/// Two terms:
///   Pawn shield: checks the three squares one rank in front of the king.
///     Missing pawn  → -20 cp; pawn pushed one rank → -10 cp; pawn on shield → 0.
///   Open files: checks the king's file and its two neighbours.
///     Fully open (no pawns either side) → -25 cp; semi-open (no friendly pawn) → -10 cp.
///
/// Only applied when the king is near its own back rank (ranks 1-2 for White, 7-8 for Black),
/// i.e. the pawn-shield concept applies.  A king in the centre gets no penalty here.
fn king_safety_penalty(pos: &Position, color: Color) -> i32 {
    let king_sq   = pos.bbs.pieces(color, PieceKind::King).lsb();
    let king_rank = king_sq.rank();
    let king_file = king_sq.file();

    let near_back_rank = match color {
        Color::White => king_rank <= 1,
        Color::Black => king_rank >= 6,
    };
    if !near_back_rank { return 0; }

    let friendly_pawns = pos.bbs.pieces(color, PieceKind::Pawn);
    let all_pawns      = friendly_pawns | pos.bbs.pieces(color.opposite(), PieceKind::Pawn);

    // One rank toward the opponent.
    let shield_rank:   u8 = match color { Color::White => king_rank + 1, Color::Black => king_rank - 1 };
    // One rank further (pawn pushed once off the shield square).
    let advanced_rank: u8 = match color { Color::White => shield_rank + 1, Color::Black => shield_rank - 1 };

    let file_lo = king_file.saturating_sub(1);
    let file_hi = (king_file + 1).min(7);

    let mut penalty = 0i32;

    for f in file_lo..=file_hi {
        // --- Pawn shield ---
        let shield_sq   = Square::from_file_rank(f, shield_rank);
        let advanced_sq = Square::from_file_rank(f, advanced_rank);

        if friendly_pawns.contains(shield_sq) {
            // pawn on the shield square — ideal, no penalty
        } else if friendly_pawns.contains(advanced_sq) {
            penalty -= 10; // pawn pushed one rank past the shield
        } else {
            penalty -= 20; // pawn gone entirely
        }

        // --- Open file toward the king ---
        let fmask = file_mask(f);
        if (all_pawns & fmask).is_empty() {
            penalty -= 25; // fully open: rooks/queens have a highway to the king
        } else if (friendly_pawns & fmask).is_empty() {
            penalty -= 10; // semi-open: enemy pawn present but no friendly blocker
        }
    }

    penalty
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

    // --- King safety tests ---

    #[test]
    fn king_safety_full_shield_no_penalty() {
        // White king g1, pawns f2/g2/h2 — complete pawn shield.
        let pos = Position::from_fen("6k1/8/8/8/8/8/5PPP/6K1 w - - 0 1").unwrap();
        assert_eq!(king_safety_penalty(&pos, Color::White), 0);
    }

    #[test]
    fn king_safety_advanced_pawn_small_penalty() {
        // White king g1, f2/h2 intact, g-pawn on g3 (pushed one rank off shield).
        let pos = Position::from_fen("6k1/8/8/8/8/6P1/5P1P/6K1 w - - 0 1").unwrap();
        let p = king_safety_penalty(&pos, Color::White);
        assert!(p < 0,  "advanced g-pawn should incur a penalty");
        assert!(p > -30, "penalty should be small for one advanced pawn");
    }

    #[test]
    fn king_safety_missing_pawns_larger_penalty() {
        // White king g1, only g2 pawn — f and h pawns gone.
        let pos_partial = Position::from_fen("6k1/8/8/8/8/8/6P1/6K1 w - - 0 1").unwrap();
        let pos_full    = Position::from_fen("6k1/8/8/8/8/8/5PPP/6K1 w - - 0 1").unwrap();
        assert!(king_safety_penalty(&pos_partial, Color::White)
              < king_safety_penalty(&pos_full,    Color::White),
              "missing two shield pawns should give a worse penalty");
    }

    #[test]
    fn king_safety_open_file_adds_penalty() {
        // White king g1, f2/h2 present but g-file open (no g-pawn at all).
        let pos_open   = Position::from_fen("6k1/8/8/8/8/8/5P1P/6K1 w - - 0 1").unwrap();
        let pos_closed = Position::from_fen("6k1/8/8/8/8/8/5PPP/6K1 w - - 0 1").unwrap();
        assert!(king_safety_penalty(&pos_open,   Color::White)
              < king_safety_penalty(&pos_closed, Color::White),
              "open g-file should add penalty beyond missing pawn alone");
    }

    #[test]
    fn king_safety_starting_position_symmetric() {
        let pos = Position::starting_position();
        assert_eq!(king_safety_penalty(&pos, Color::White),
                   king_safety_penalty(&pos, Color::Black));
    }

    #[test]
    fn king_safety_centre_king_no_penalty() {
        // King marches to the centre — shield concept does not apply.
        let pos = Position::from_fen("8/8/8/4K3/8/8/8/7k w - - 0 1").unwrap();
        assert_eq!(king_safety_penalty(&pos, Color::White), 0);
    }

    #[test]
    fn king_safety_evaluate_still_zero_at_start() {
        assert_eq!(evaluate(&Position::starting_position()), 0);
    }

    // --- Passed pawn tests ---

    #[test]
    fn passed_pawn_no_passers_starting_position() {
        // Both sides fully blocked — no passed pawns.
        assert_eq!(passed_pawn_bonus(&Position::starting_position(), Color::White), 0);
        assert_eq!(passed_pawn_bonus(&Position::starting_position(), Color::Black), 0);
    }

    #[test]
    fn passed_pawn_white_passer_on_rank5() {
        // White pawn on e5, no Black pawns on d/e/f files ahead — clearly passed.
        let pos = Position::from_fen("4k3/8/8/4P3/8/8/8/4K3 w - - 0 1").unwrap();
        let bonus = passed_pawn_bonus(&pos, Color::White);
        assert_eq!(bonus, PASSED_PAWN_BONUS[4], "e5 is rank index 4, bonus should be 35");
    }

    #[test]
    fn passed_pawn_bonus_scales_with_rank() {
        // Same passer on rank 6 should score more than rank 4.
        let pos_r6 = Position::from_fen("4k3/8/4P3/8/8/8/8/4K3 w - - 0 1").unwrap();
        let pos_r4 = Position::from_fen("4k3/8/8/8/4P3/8/8/4K3 w - - 0 1").unwrap();
        assert!(passed_pawn_bonus(&pos_r6, Color::White)
              > passed_pawn_bonus(&pos_r4, Color::White));
    }

    #[test]
    fn passed_pawn_black_passer_detected() {
        // Black pawn on e4 — no White pawns on d/e/f files ahead for Black.
        let pos = Position::from_fen("4k3/8/8/8/4p3/8/8/4K3 w - - 0 1").unwrap();
        let bonus = passed_pawn_bonus(&pos, Color::Black);
        assert_eq!(bonus, PASSED_PAWN_BONUS[4], "e4 from Black's view is rank index 4");
    }

    #[test]
    fn passed_pawn_blocked_by_piece_still_passes() {
        // White passer on e5 blocked by Black rook on e6 — rook is not a pawn, still passed.
        let pos = Position::from_fen("4k3/8/4r3/4P3/8/8/8/4K3 w - - 0 1").unwrap();
        assert!(passed_pawn_bonus(&pos, Color::White) > 0);
    }

    #[test]
    fn passed_pawn_adjacent_enemy_same_rank_does_not_block() {
        // Black pawn on d4, White pawn on e4 — d4 is beside e4, not ahead; e4 is still passed.
        let pos = Position::from_fen("4k3/8/8/8/3pP3/8/8/4K3 w - - 0 1").unwrap();
        assert!(passed_pawn_bonus(&pos, Color::White) > 0,
            "adjacent same-rank enemy pawn should not block the passer");
    }

    #[test]
    fn passed_pawn_adjacent_enemy_ahead_does_block() {
        // Black pawn on d5 is ahead-adjacent to White pawn on e4 — should block.
        let pos = Position::from_fen("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(passed_pawn_bonus(&pos, Color::White), 0,
            "enemy pawn on adjacent file ahead should block the passer");
    }

    #[test]
    fn passed_pawn_evaluate_still_zero_at_start() {
        assert_eq!(evaluate(&Position::starting_position()), 0);
    }

    #[test]
    fn king_safety_exposed_king_scores_lower() {
        // Equal material (3 pawns each); White king sheltered on g1, Black king
        // exposed on d8 with its pawns on the wrong wing.
        let pos = Position::from_fen("3k4/5ppp/8/8/8/8/5PPP/6K1 w - - 0 1").unwrap();
        assert!(evaluate(&pos) > 0,
            "White with sheltered king should outscore exposed Black king");
    }

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
