use crate::bitboard::{file_mask, front_fill, knight_attacks, Bitboard, BISHOP_RAYS, ROOK_RAYS};
use crate::position::Position;
use crate::types::{Color, PieceKind, Square};

/// Evaluate the position in centipawns from White's perspective.
/// Positive = White is better, negative = Black is better.
pub fn evaluate(pos: &Position) -> i32 {
    let mut score = 0i32;
    let phase = game_phase(pos);

    for color in [Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        for kind in [PieceKind::Pawn, PieceKind::Knight, PieceKind::Bishop,
                     PieceKind::Rook, PieceKind::Queen, PieceKind::King] {
            let mut bb = pos.bbs.pieces(color, kind);
            while !bb.is_empty() {
                let sq = bb.pop_lsb();
                score += sign * (material_value(kind) + piece_square_bonus(kind, color, sq, phase));
            }
        }
        score += sign * king_safety_penalty(pos, color);
        score += sign * passed_pawn_bonus(pos, color);
        score += sign * pawn_structure_penalty(pos, color);
        score += sign * rook_bonus(pos, color);
        score += sign * mobility_bonus(pos, color);
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

/// Pawn structure penalty for `color` (always <= 0).
///
/// Two terms:
///   Doubled pawns: more than one friendly pawn on the same file.
///     Penalty of DOUBLED_PAWN_PENALTY per extra pawn beyond the first.
///   Isolated pawns: a pawn with no friendly pawns on either adjacent file.
///     Penalty of ISOLATED_PAWN_PENALTY per isolated pawn.
fn pawn_structure_penalty(pos: &Position, color: Color) -> i32 {
    let pawns = pos.bbs.pieces(color, PieceKind::Pawn);
    if pawns.is_empty() {
        return 0;
    }

    let mut penalty = 0i32;

    for file in 0u8..8 {
        let on_file = pawns & file_mask(file);
        if on_file.is_empty() {
            continue;
        }

        let count = on_file.popcount();
        if count > 1 {
            penalty += (count - 1) as i32 * DOUBLED_PAWN_PENALTY;
        }

        let left  = if file > 0 { pawns & file_mask(file - 1) } else { crate::bitboard::Bitboard::EMPTY };
        let right = if file < 7 { pawns & file_mask(file + 1) } else { crate::bitboard::Bitboard::EMPTY };
        if left.is_empty() && right.is_empty() {
            penalty += count as i32 * ISOLATED_PAWN_PENALTY;
        }
    }

    penalty
}

const DOUBLED_PAWN_PENALTY:  i32 = -20;
const ISOLATED_PAWN_PENALTY: i32 = -15;

/// Bonus for rooks on open/semi-open files and on the 7th rank.
///
/// Fully open file (no pawns either color): +20 cp.
/// Semi-open file (no friendly pawns, enemy pawns present): +10 cp.
/// 7th rank (rank 7 for White, rank 2 for Black): +25 cp, stacks with file bonus.
fn rook_bonus(pos: &Position, color: Color) -> i32 {
    let friendly_pawns = pos.bbs.pieces(color, PieceKind::Pawn);
    let all_pawns = friendly_pawns | pos.bbs.pieces(color.opposite(), PieceKind::Pawn);
    let seventh_rank: u8 = match color { Color::White => 6, Color::Black => 1 };

    let mut bonus = 0i32;
    let mut rooks = pos.bbs.pieces(color, PieceKind::Rook);
    while !rooks.is_empty() {
        let sq = rooks.pop_lsb();
        let file = sq.file();
        if (all_pawns & file_mask(file)).is_empty() {
            bonus += ROOK_OPEN_FILE_BONUS;
        } else if (friendly_pawns & file_mask(file)).is_empty() {
            bonus += ROOK_SEMI_OPEN_FILE_BONUS;
        }
        if sq.rank() == seventh_rank {
            bonus += ROOK_SEVENTH_RANK_BONUS;
        }
    }
    bonus
}

const ROOK_OPEN_FILE_BONUS:    i32 = 20;
const ROOK_SEMI_OPEN_FILE_BONUS: i32 = 10;
const ROOK_SEVENTH_RANK_BONUS:  i32 = 25;

const KNIGHT_MOBILITY_BONUS: i32 = 4;
const BISHOP_MOBILITY_BONUS: i32 = 3;
const ROOK_MOBILITY_BONUS:   i32 = 2;
const QUEEN_MOBILITY_BONUS:  i32 = 1;

/// Walk one ray direction from `from`, counting squares reachable through empty space.
/// Stops after hitting any occupant (that square counts; one past a blocker does not).
fn slider_ray_attacks(from: Square, occ: Bitboard, shifts: &[fn(Bitboard) -> Bitboard]) -> Bitboard {
    let mut attacks = Bitboard::EMPTY;
    for &shift in shifts {
        let mut ray = Bitboard::from_square(from);
        loop {
            ray = shift(ray);
            if ray.is_empty() { break; }
            attacks = attacks | ray;
            if !(ray & occ).is_empty() { break; }
        }
    }
    attacks
}

/// Mobility bonus for `color` in centipawns.
/// Counts squares each piece can reach (attacks & not own pieces).
fn mobility_bonus(pos: &Position, color: Color) -> i32 {
    let occ      = pos.bbs.occupancy();
    let friendly = pos.bbs.color_occupancy(color);
    let mut bonus = 0i32;

    let mut knights = pos.bbs.pieces(color, PieceKind::Knight);
    while !knights.is_empty() {
        let sq = knights.pop_lsb();
        let attacks = knight_attacks()[sq.index() as usize] & !friendly;
        bonus += attacks.popcount() as i32 * KNIGHT_MOBILITY_BONUS;
    }

    let mut bishops = pos.bbs.pieces(color, PieceKind::Bishop);
    while !bishops.is_empty() {
        let sq = bishops.pop_lsb();
        let attacks = slider_ray_attacks(sq, occ, &BISHOP_RAYS) & !friendly;
        bonus += attacks.popcount() as i32 * BISHOP_MOBILITY_BONUS;
    }

    let mut rooks = pos.bbs.pieces(color, PieceKind::Rook);
    while !rooks.is_empty() {
        let sq = rooks.pop_lsb();
        let attacks = slider_ray_attacks(sq, occ, &ROOK_RAYS) & !friendly;
        bonus += attacks.popcount() as i32 * ROOK_MOBILITY_BONUS;
    }

    let mut queens = pos.bbs.pieces(color, PieceKind::Queen);
    while !queens.is_empty() {
        let sq = queens.pop_lsb();
        let rook_part   = slider_ray_attacks(sq, occ, &ROOK_RAYS)   & !friendly;
        let bishop_part = slider_ray_attacks(sq, occ, &BISHOP_RAYS) & !friendly;
        bonus += (rook_part | bishop_part).popcount() as i32 * QUEEN_MOBILITY_BONUS;
    }

    bonus
}

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

pub fn material_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn   =>    100,
        PieceKind::Knight =>    320,
        PieceKind::Bishop =>    330,
        PieceKind::Rook   =>    500,
        PieceKind::Queen  =>    900,
        PieceKind::King   => 20_000,
    }
}

fn piece_square_bonus(kind: PieceKind, color: Color, sq: Square, phase: i32) -> i32 {
    let idx = match color {
        Color::White => (7 - sq.rank() as usize) * 8 + sq.file() as usize,
        Color::Black =>      sq.rank() as usize  * 8 + sq.file() as usize,
    };
    if kind == PieceKind::King {
        // Blend MG and EG king tables based on game phase (0=opening, 256=endgame).
        let mg = KING_MG_TABLE[idx];
        let eg = KING_EG_TABLE[idx];
        return (mg * (256 - phase) + eg * phase) / 256;
    }
    let table: &[i32; 64] = match kind {
        PieceKind::Pawn   => &PAWN_TABLE,
        PieceKind::Knight => &KNIGHT_TABLE,
        PieceKind::Bishop => &BISHOP_TABLE,
        PieceKind::Rook   => &ROOK_TABLE,
        PieceKind::Queen  => &QUEEN_TABLE,
        PieceKind::King   => unreachable!(),
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
const KING_MG_TABLE: [i32; 64] = [
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -20,-30,-30,-40,-40,-30,-30,-20,
    -10,-20,-20,-20,-20,-20,-20,-10,
     20, 20,  0,  0,  0,  0, 20, 20,
     20, 30, 10,  0,  0, 10, 30, 20,
];

#[rustfmt::skip]
const KING_EG_TABLE: [i32; 64] = [
    -50,-40,-30,-20,-20,-30,-40,-50,
    -30,-20,-10,  0,  0,-10,-20,-30,
    -30,-10, 20, 30, 30, 20,-10,-30,
    -30,-10, 30, 40, 40, 30,-10,-30,
    -30,-10, 30, 40, 40, 30,-10,-30,
    -30,-10, 20, 30, 30, 20,-10,-30,
    -30,-30,  0,  0,  0,  0,-30,-30,
    -50,-30,-30,-30,-30,-30,-30,-50,
];

// Phase weights per piece kind: [Pawn, Knight, Bishop, Rook, Queen, King]
const PHASE_WEIGHTS: [i32; 6] = [0, 1, 1, 2, 4, 0];
// Total phase when all pieces are present (4 knights + 4 bishops + 4 rooks + 2 queens)
const TOTAL_PHASE: i32 = 4 * 1 + 4 * 1 + 4 * 2 + 2 * 4; // = 24

/// Returns 0 (opening/middlegame) to 256 (full endgame).
/// Decreases as major pieces are captured.
pub fn game_phase(pos: &Position) -> i32 {
    let mut phase = 0i32;
    for color in [Color::White, Color::Black] {
        for kind in [PieceKind::Pawn, PieceKind::Knight, PieceKind::Bishop,
                     PieceKind::Rook, PieceKind::Queen, PieceKind::King] {
            let count = pos.bbs.pieces(color, kind).popcount() as i32;
            phase += count * PHASE_WEIGHTS[kind as usize];
        }
    }
    let phase = phase.min(TOTAL_PHASE); // clamp (shouldn't exceed, but guard promotions)
    (TOTAL_PHASE - phase) * 256 / TOTAL_PHASE
}

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

    // --- pawn structure tests ---

    #[test]
    fn pawn_structure_no_weaknesses() {
        // Consecutive White pawns on d2, e2, f2, g2 — each has a neighbor, none isolated or doubled.
        let pos = Position::from_fen("4k3/8/8/8/8/8/3PPPP1/4K3 w - - 0 1").unwrap();
        assert_eq!(pawn_structure_penalty(&pos, Color::White), 0,
            "consecutive pawns should have no structural penalty");
    }

    #[test]
    fn pawn_structure_doubled_pawn_detected() {
        // White pawns on d2, e2, e4 — doubled on e-file; d2 makes e-file pawns non-isolated.
        let pos = Position::from_fen("4k3/8/8/8/4P3/8/3PP3/4K3 w - - 0 1").unwrap();
        assert_eq!(pawn_structure_penalty(&pos, Color::White), DOUBLED_PAWN_PENALTY,
            "two pawns on same file should give one doubled penalty");
    }

    #[test]
    fn pawn_structure_tripled_pawn_detected() {
        // White pawns on d2, e2, e4, e6 — tripled on e-file; d2 makes e-file pawns non-isolated.
        let pos = Position::from_fen("4k3/8/4P3/8/4P3/8/3PP3/4K3 w - - 0 1").unwrap();
        assert_eq!(pawn_structure_penalty(&pos, Color::White), 2 * DOUBLED_PAWN_PENALTY,
            "three pawns on same file should give two doubled penalties");
    }

    #[test]
    fn pawn_structure_isolated_pawn_detected() {
        // White pawn on a5 only — a-file pawn, no pawns on b-file
        let pos = Position::from_fen("4k3/8/8/P7/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(pawn_structure_penalty(&pos, Color::White), ISOLATED_PAWN_PENALTY,
            "lone pawn on a-file with no b-file neighbor should be isolated");
    }

    #[test]
    fn pawn_structure_doubled_and_isolated() {
        // Two White pawns on a-file, no White pawns on b-file: doubled + isolated
        let pos = Position::from_fen("4k3/8/8/P7/P7/8/8/4K3 w - - 0 1").unwrap();
        let expected = DOUBLED_PAWN_PENALTY + 2 * ISOLATED_PAWN_PENALTY;
        assert_eq!(pawn_structure_penalty(&pos, Color::White), expected,
            "doubled isolated pawns should accumulate both penalties");
    }

    #[test]
    fn pawn_structure_starting_position_zero() {
        // Starting position is symmetric — both sides have identical structure
        let pos = Position::starting_position();
        assert_eq!(pawn_structure_penalty(&pos, Color::White),
                   pawn_structure_penalty(&pos, Color::Black),
            "starting position pawn structure should be symmetric");
        assert_eq!(evaluate(&pos), 0, "starting position overall eval should be zero");
    }

    #[test]
    fn pawn_structure_black_weakness_is_positive() {
        // White: d2, e2 (adjacent, no penalty). Black: e6, e7 (doubled on e-file).
        let pos = Position::from_fen("4k3/4p3/4p3/8/8/8/3PP3/4K3 w - - 0 1").unwrap();
        let w_pen = pawn_structure_penalty(&pos, Color::White);
        let b_pen = pawn_structure_penalty(&pos, Color::Black);
        assert!(b_pen < w_pen, "Black with doubled pawn should have worse structure penalty");
    }

    // --- rook bonus tests ---

    #[test]
    fn rook_bonus_closed_file_no_bonus() {
        // White rook on e1, White pawn on e2 — closed file, no bonus.
        // King on g1 to keep rook on e-file (4R1K1 = a1-d1 empty, e1=R, f1 empty, g1=K, h1 empty).
        let pos = Position::from_fen("4k3/8/8/8/8/8/4P3/4R1K1 w - - 0 1").unwrap();
        assert_eq!(rook_bonus(&pos, Color::White), 0, "rook behind own pawn gets no bonus");
    }

    #[test]
    fn rook_bonus_semi_open_file() {
        // White rook on e1, Black pawn on e5, no White pawn on e-file — semi-open.
        let pos = Position::from_fen("4k3/8/8/4p3/8/8/8/4R1K1 w - - 0 1").unwrap();
        assert_eq!(rook_bonus(&pos, Color::White), ROOK_SEMI_OPEN_FILE_BONUS,
            "rook on semi-open file (enemy pawn only) should get semi-open bonus");
    }

    #[test]
    fn rook_bonus_fully_open_file() {
        // White rook on e1, no pawns on e-file at all — fully open.
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4KR2 w - - 0 1").unwrap();
        assert_eq!(rook_bonus(&pos, Color::White), ROOK_OPEN_FILE_BONUS,
            "rook on fully open file should get open file bonus");
    }

    #[test]
    fn rook_bonus_seventh_rank_white() {
        // White rook on e7 — on the 7th rank; e-file has pawns so no file bonus.
        let pos = Position::from_fen("4k3/4R3/8/8/8/8/4P3/4K3 w - - 0 1").unwrap();
        assert_eq!(rook_bonus(&pos, Color::White), ROOK_SEVENTH_RANK_BONUS,
            "White rook on rank 7 should get seventh-rank bonus");
    }

    #[test]
    fn rook_bonus_open_file_and_seventh_rank_stack() {
        // White rook on e7, no pawns on e-file — gets both bonuses.
        let pos = Position::from_fen("4k3/4R3/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(rook_bonus(&pos, Color::White), ROOK_OPEN_FILE_BONUS + ROOK_SEVENTH_RANK_BONUS,
            "open file and seventh rank bonuses should stack");
    }

    #[test]
    fn rook_bonus_black_second_rank() {
        // Black rook on e2 (rank index 1 = rank 2) — 7th rank equivalent for Black.
        let pos = Position::from_fen("4k3/8/8/8/8/8/4r3/4K3 w - - 0 1").unwrap();
        assert_eq!(rook_bonus(&pos, Color::Black), ROOK_OPEN_FILE_BONUS + ROOK_SEVENTH_RANK_BONUS,
            "Black rook on rank 2 open file should get both bonuses");
    }

    #[test]
    fn rook_bonus_starting_position_zero() {
        let pos = Position::starting_position();
        assert_eq!(rook_bonus(&pos, Color::White), 0);
        assert_eq!(rook_bonus(&pos, Color::Black), 0);
        assert_eq!(evaluate(&pos), 0, "starting position should still be equal");
    }

    // --- Endgame phase tests ---

    #[test]
    fn game_phase_starting_position_is_zero() {
        // Full piece complement → pure middlegame.
        let pos = Position::starting_position();
        assert_eq!(game_phase(&pos), 0, "starting position should have phase 0 (middlegame)");
    }

    #[test]
    fn game_phase_kings_only_is_256() {
        // Only kings remain → pure endgame.
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(game_phase(&pos), 256, "kings-only should have phase 256 (endgame)");
    }

    #[test]
    fn game_phase_partial_decreases() {
        // A few pieces left → somewhere between 0 and 256.
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1").unwrap();
        let phase = game_phase(&pos);
        assert!(phase > 0 && phase < 256, "two rooks and kings should yield intermediate phase, got {phase}");
    }

    #[test]
    fn king_prefers_corner_in_middlegame() {
        // In MG (phase=0), KING_MG_TABLE rewards back-rank corner (castled position).
        let mg_corner  = piece_square_bonus(PieceKind::King, Color::White, Square::from_file_rank(6, 0), 0);
        let mg_center  = piece_square_bonus(PieceKind::King, Color::White, Square::from_file_rank(4, 4), 0);
        assert!(mg_corner > mg_center,
            "MG king should prefer castled corner over center, corner={mg_corner} center={mg_center}");
    }

    #[test]
    fn king_prefers_center_in_endgame() {
        // In EG (phase=256), KING_EG_TABLE rewards central squares.
        let eg_center = piece_square_bonus(PieceKind::King, Color::White, Square::from_file_rank(3, 3), 256);
        let eg_corner = piece_square_bonus(PieceKind::King, Color::White, Square::from_file_rank(0, 0), 256);
        assert!(eg_center > eg_corner,
            "EG king should prefer center over corner, center={eg_center} corner={eg_corner}");
    }

    #[test]
    fn king_phase_blend_is_between_mg_and_eg() {
        // At phase=128 (half endgame), bonus should be between pure MG and EG values.
        let sq = Square::from_file_rank(3, 3); // d4 — good EG square, bad MG square
        let mg = piece_square_bonus(PieceKind::King, Color::White, sq, 0);
        let eg = piece_square_bonus(PieceKind::King, Color::White, sq, 256);
        let blend = piece_square_bonus(PieceKind::King, Color::White, sq, 128);
        let lo = mg.min(eg);
        let hi = mg.max(eg);
        assert!(blend >= lo && blend <= hi,
            "blended bonus {blend} should be between MG {mg} and EG {eg}");
    }

    // --- Mobility tests ---

    #[test]
    fn mobility_starting_position_is_symmetric() {
        // Both sides have identical mobility in the starting position.
        let pos = Position::starting_position();
        let w = mobility_bonus(&pos, Color::White);
        let b = mobility_bonus(&pos, Color::Black);
        assert_eq!(w, b, "mobility should be equal for both sides at start: white={w} black={b}");
        assert_eq!(evaluate(&pos), 0, "evaluate must still return 0 from starting position");
    }

    #[test]
    fn mobility_knight_rim_vs_center() {
        // Knight on a1 has 2 moves; knight in the centre (e.g. d4) has up to 8.
        // White knight on a1, kings far away.
        let pos_rim = Position::from_fen("4k3/8/8/8/8/8/8/N3K3 w - - 0 1").unwrap();
        let pos_ctr = Position::from_fen("4k3/8/8/8/3N4/8/8/4K3 w - - 0 1").unwrap();
        let rim = mobility_bonus(&pos_rim, Color::White);
        let ctr = mobility_bonus(&pos_ctr, Color::White);
        assert!(ctr > rim, "central knight mobility {ctr} should exceed rim knight mobility {rim}");
    }

    #[test]
    fn mobility_locked_bishop_vs_open_bishop() {
        // Bishop blocked by its own pawns vs a bishop on an open diagonal.
        // Locked: White bishop c1, pawns on b2 and d2 (fully blocked).
        let pos_locked = Position::from_fen("4k3/8/8/8/8/8/1P1P4/2B1K3 w - - 0 1").unwrap();
        // Open: White bishop c1 with open diagonals.
        let pos_open   = Position::from_fen("4k3/8/8/8/8/8/8/2B1K3 w - - 0 1").unwrap();
        let locked = mobility_bonus(&pos_locked, Color::White);
        let open   = mobility_bonus(&pos_open,   Color::White);
        assert!(open > locked, "open bishop mobility {open} should exceed locked bishop mobility {locked}");
    }
}
