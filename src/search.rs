use std::time::Instant;

use crate::eval::{evaluate, material_value};
use crate::movegen::{generate_legal_moves, Move, MoveKind};
use crate::position::Position;
use crate::tt::{Bound, TranspositionTable};
use crate::types::{Color, PieceKind};

const INFINITY: i32 = 1_000_000;
const MATE_SCORE: i32 = 100_000;
const MAX_PLY: usize = 64;

/// Two quiet moves per ply that caused a beta cutoff — tried before other quiet moves.
type KillerTable = [[Option<Move>; 2]; MAX_PLY];

pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: i32,
    pub depth: u32,
    pub nodes: u64,
    /// Top-N moves with scores (side-to-move perspective), sorted best-first.
    /// Populated at the final depth of iterative deepening.
    pub candidates: Vec<(Move, i32)>,
}

/// Run quiescence search from the current position and return the score from White's perspective.
/// Used for standalone eval display when no full search result is available.
pub fn quiescence_eval(pos: &Position, qdepth: u32) -> i32 {
    let mut nodes = 0u64;
    let stm_score = quiescence(pos, -INFINITY, INFINITY, &mut nodes, qdepth);
    if pos.side_to_move == Color::White { stm_score } else { -stm_score }
}

/// Search to `max_depth` using iterative deepening with alpha-beta pruning.
/// Returns the best move found, its score (side-to-move perspective), and the top `n` candidates.
///
/// `tt` persists across calls so knowledge from earlier moves and shallower depths is reused.
/// If `deadline` is set, stops after the first depth that completes past the deadline.
pub fn search(
    pos: &Position,
    max_depth: u32,
    game_history: &[u64],
    qdepth: u32,
    n: usize,
    deadline: Option<Instant>,
    tt: &mut TranspositionTable,
) -> SearchResult {
    let mut result = SearchResult { best_move: None, score: 0, depth: 0, nodes: 0, candidates: Vec::new() };

    let mut base_history = Vec::from(game_history);
    base_history.push(pos.hash);
    let mut killers: KillerTable = [[None; 2]; MAX_PLY];

    for depth in 1..=max_depth {
        let mut nodes = 0u64;
        let mut history = base_history.clone();
        let (score, mv, cands) = negamax_root(pos, depth, &mut nodes, &mut history, qdepth, n, tt, &mut killers);
        result.best_move = mv;
        result.score = score;
        result.depth = depth;
        result.nodes += nodes;
        result.candidates = cands;

        if deadline.map_or(false, |d| Instant::now() >= d) {
            break;
        }
    }

    result
}

fn negamax_root(
    pos: &Position,
    depth: u32,
    nodes: &mut u64,
    history: &mut Vec<u64>,
    qdepth: u32,
    n: usize,
    tt: &mut TranspositionTable,
    killers: &mut KillerTable,
) -> (i32, Option<Move>, Vec<(Move, i32)>) {
    let mut moves = generate_legal_moves(pos);

    if moves.is_empty() {
        let score = if pos.is_in_check(pos.side_to_move) { -MATE_SCORE } else { 0 };
        return (score, None, Vec::new());
    }

    // Use the TT move from the previous iteration (or a prior game search) as the first move
    // tried at the root. This is the main source of move ordering improvement from the TT.
    let tt_move = tt.probe(pos.hash).and_then(|e| e.mv);
    order_moves(pos, &mut moves, tt_move, &killers[0]);

    let mut scored: Vec<(Move, i32)> = Vec::with_capacity(moves.len());
    let mut alpha = -INFINITY;

    for mv in &moves {
        let after = pos.make_move(mv);
        history.push(after.hash);
        let score = -negamax(&after, depth - 1, -INFINITY, -alpha, 1, nodes, history, qdepth, tt, killers, false);
        history.pop();
        scored.push((*mv, score));
        if score > alpha {
            alpha = score;
        }
    }

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    let best_move = scored.first().map(|(mv, _)| *mv);

    // Store root position as exact (we searched all moves).
    tt.store(pos.hash, alpha, depth as u8, Bound::Exact, best_move);

    scored.truncate(n);
    (alpha, best_move, scored)
}

fn negamax(
    pos: &Position,
    depth: u32,
    mut alpha: i32,
    beta: i32,
    ply: u32,
    nodes: &mut u64,
    history: &mut Vec<u64>,
    qdepth: u32,
    tt: &mut TranspositionTable,
    killers: &mut KillerTable,
    last_was_null: bool,
) -> i32 {
    *nodes += 1;

    // pos.hash was pushed by the caller; >= 2 occurrences means we've been here — draw.
    let reps = history.iter().filter(|&&h| h == pos.hash).count();
    if reps >= 2 { return 0; }

    if depth == 0 {
        return quiescence(pos, alpha, beta, nodes, qdepth);
    }

    if pos.halfmove_clock >= 100 { return 0; }

    // --- Transposition table probe ---
    let original_alpha = alpha;
    let mut beta = beta;
    let mut tt_move: Option<Move> = None;

    if let Some(entry) = tt.probe(pos.hash) {
        tt_move = entry.mv; // use for move ordering even when depth is insufficient
        if entry.depth >= depth as u8 {
            match entry.bound {
                Bound::Exact => return entry.score,
                Bound::Lower => alpha = alpha.max(entry.score),
                Bound::Upper => beta  = beta.min(entry.score),
            }
            if alpha >= beta { return entry.score; }
        }
    }

    let mut moves = generate_legal_moves(pos);

    if moves.is_empty() {
        return if pos.is_in_check(pos.side_to_move) {
            ply as i32 - MATE_SCORE // prefer shorter mates
        } else {
            0 // stalemate
        };
    }

    // --- Null move pruning ---
    // If giving the opponent a free tempo still fails to save them, prune.
    // Guards: not in check (can't pass), not after another null move, depth >= 3,
    // and we have at least one non-pawn piece (avoids zugzwang in king+pawn endings).
    let in_check = pos.is_in_check(pos.side_to_move);
    let stm = pos.side_to_move;
    let has_pieces = !(pos.bbs.pieces(stm, PieceKind::Knight)
        | pos.bbs.pieces(stm, PieceKind::Bishop)
        | pos.bbs.pieces(stm, PieceKind::Rook)
        | pos.bbs.pieces(stm, PieceKind::Queen)).is_empty();

    if !last_was_null && !in_check && depth >= 3 && has_pieces {
        let null_pos = make_null_move(pos);
        history.push(null_pos.hash);
        let null_score = -negamax(&null_pos, depth - 1 - 2, -beta, -beta + 1,
                                   ply + 1, nodes, history, qdepth, tt, killers, true);
        history.pop();
        if null_score >= beta {
            return beta;
        }
    }

    let ply_idx = (ply as usize).min(MAX_PLY - 1);
    order_moves(pos, &mut moves, tt_move, &killers[ply_idx]);

    let mut best_move: Option<Move> = None;

    for (move_index, mv) in moves.iter().enumerate() {
        let is_quiet = !pos.bbs.occupancy().contains(mv.to)
            && mv.kind != MoveKind::EnPassant
            && !matches!(mv.kind, MoveKind::Promotion(_));
        let is_killer = killers[ply_idx][0] == Some(*mv) || killers[ply_idx][1] == Some(*mv);

        let after = pos.make_move(mv);
        history.push(after.hash);

        // Late Move Reductions: quiet non-killer moves late in the list are statistically
        // unlikely to be best, so search them at reduced depth first. Only re-search at
        // full depth if the reduced score raises alpha.
        let score = if move_index >= 3 && depth >= 3 && !in_check && is_quiet && !is_killer {
            let reduced = -negamax(&after, depth - 2, -alpha - 1, -alpha,
                                   ply + 1, nodes, history, qdepth, tt, killers, false);
            if reduced > alpha {
                -negamax(&after, depth - 1, -beta, -alpha, ply + 1, nodes, history, qdepth, tt, killers, false)
            } else {
                reduced
            }
        } else {
            -negamax(&after, depth - 1, -beta, -alpha, ply + 1, nodes, history, qdepth, tt, killers, false)
        };

        history.pop();

        if score >= beta {
            if is_quiet {
                killers[ply_idx][1] = killers[ply_idx][0];
                killers[ply_idx][0] = Some(*mv);
            }
            tt.store(pos.hash, score, depth as u8, Bound::Lower, Some(*mv));
            return beta;
        }
        if score > alpha {
            alpha = score;
            best_move = Some(*mv);
        }
    }

    // Store result. Exact if we raised alpha; Upper bound if all moves failed low.
    let bound = if alpha > original_alpha { Bound::Exact } else { Bound::Upper };
    tt.store(pos.hash, alpha, depth as u8, bound, best_move);

    alpha
}

fn quiescence(pos: &Position, mut alpha: i32, beta: i32, nodes: &mut u64, qdepth: u32) -> i32 {
    *nodes += 1;

    let stand_pat = eval_from_stm(pos);
    if stand_pat >= beta { return beta; }
    if stand_pat > alpha { alpha = stand_pat; }

    if qdepth == 0 { return alpha; }

    let captures: Vec<_> = generate_legal_moves(pos)
        .into_iter()
        .filter(|mv| {
            pos.bbs.occupancy().contains(mv.to)
                || mv.kind == MoveKind::EnPassant
                || matches!(mv.kind, MoveKind::Promotion(_))
        })
        .collect();

    for mv in &captures {
        let after = pos.make_move(mv);
        let score = -quiescence(&after, -beta, -alpha, nodes, qdepth - 1);
        if score >= beta { return beta; }
        if score > alpha { alpha = score; }
    }

    alpha
}

/// Make a null move: flip side to move and clear en passant without moving a piece.
/// Used by null move pruning. Not valid in check or zugzwang-prone positions.
fn make_null_move(pos: &Position) -> Position {
    let mut p = pos.clone();
    p.side_to_move = pos.side_to_move.opposite();
    p.en_passant = None;
    p.halfmove_clock += 1;
    p.hash = p.compute_hash();
    p
}

fn eval_from_stm(pos: &Position) -> i32 {
    let raw = evaluate(pos);
    if pos.side_to_move == Color::White { raw } else { -raw }
}

/// Order moves for alpha-beta efficiency.
/// Priority (lower sort key = searched first):
///   1. TT move (best from prior search at this position)
///   2. Captures by MVV-LVA (PxQ before QxP)
///   3. Promotions
///   4. Killer moves (quiet moves that caused beta cutoffs at this ply)
///   5. All other quiet moves
fn order_moves(pos: &Position, moves: &mut Vec<Move>, tt_move: Option<Move>, killers: &[Option<Move>; 2]) {
    moves.sort_by_key(|mv| {
        if tt_move == Some(*mv) {
            return -1_000_000i32;
        }
        // Captures: MVV-LVA. En passant = PxP equivalent.
        if mv.kind == MoveKind::EnPassant {
            return -(100 * 10 - 100);
        }
        if pos.bbs.occupancy().contains(mv.to) {
            let victim   = pos.bbs.piece_at(mv.to).map_or(0, |p| material_value(p.kind));
            let attacker = pos.bbs.piece_at(mv.from).map_or(100, |p| material_value(p.kind));
            return -(victim * 10 - attacker);
        }
        // Promotions before quiet moves.
        if matches!(mv.kind, MoveKind::Promotion(_)) {
            return -800i32;
        }
        // Killers: tried before ordinary quiet moves.
        if killers[0] == Some(*mv) { return 9_000i32; }
        if killers[1] == Some(*mv) { return 9_500i32; }
        // All other quiet moves.
        10_000i32
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::Position;

    fn tt() -> TranspositionTable { TranspositionTable::new() }

    #[test]
    fn starting_position_returns_a_move() {
        let pos = Position::starting_position();
        let result = search(&pos, 2, &[], 6, 3, None, &mut tt());
        assert!(result.best_move.is_some());
    }

    #[test]
    fn checkmate_position_returns_no_move() {
        let pos = Position::from_fen(
            "r1bqkb1r/pppp1Qpp/2n2n2/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 4"
        ).unwrap();
        let result = search(&pos, 1, &[], 6, 3, None, &mut tt());
        assert!(result.best_move.is_none());
        assert!(result.score <= -MATE_SCORE + 10);
    }

    #[test]
    fn finds_mate_in_one() {
        let pos = Position::from_fen("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1").unwrap();
        let result = search(&pos, 2, &[], 6, 3, None, &mut tt());
        assert!(result.best_move.is_some());
        assert!(result.score >= MATE_SCORE - 10,
            "expected mate score, got {}", result.score);
    }

    #[test]
    fn candidates_sorted_best_first() {
        let pos = Position::starting_position();
        let result = search(&pos, 2, &[], 6, 3, None, &mut tt());
        let cands = &result.candidates;
        assert!(!cands.is_empty());
        assert_eq!(cands[0].0, result.best_move.unwrap());
        for i in 1..cands.len() {
            assert!(cands[i].1 <= cands[i - 1].1);
        }
    }

    #[test]
    fn candidates_truncated_to_n() {
        let pos = Position::starting_position();
        let result = search(&pos, 2, &[], 6, 3, None, &mut tt());
        assert!(result.candidates.len() <= 3);
    }

    #[test]
    fn candidates_fewer_than_n_when_few_legal_moves() {
        let pos = Position::from_fen("7k/8/8/8/8/8/8/R5K1 b - - 0 1").unwrap();
        let result = search(&pos, 1, &[], 0, 5, None, &mut tt());
        let legal = crate::movegen::generate_legal_moves(&pos);
        assert_eq!(result.candidates.len(), legal.len().min(5));
    }

    #[test]
    fn stalemate_scores_zero() {
        let pos = Position::from_fen("k7/8/KQK5/8/8/8/8/8 b - - 0 1").unwrap();
        let result = search(&pos, 1, &[], 6, 3, None, &mut tt());
        if result.best_move.is_none() {
            assert_eq!(result.score, 0);
        }
    }

    #[test]
    fn tt_improves_node_count() {
        // With a warm TT, a second search of the same position should visit fewer nodes.
        let pos = Position::starting_position();
        let mut tt = TranspositionTable::new();
        let r1 = search(&pos, 4, &[], 0, 3, None, &mut tt);
        let r2 = search(&pos, 4, &[], 0, 3, None, &mut tt);
        assert!(r2.nodes < r1.nodes,
            "warm TT should reduce node count: first={} second={}", r1.nodes, r2.nodes);
    }

    // --- MVV-LVA ordering tests ---

    #[test]
    fn mvv_lva_pxq_before_qxp() {
        // White pawn on c5 can take Black queen on d6 (PxQ = great capture).
        // White queen on h5 can take Black pawn on h4 (QxP = risky capture).
        // MVV-LVA must order PxQ before QxP.
        let pos = Position::from_fen("4k3/8/3q4/2P4Q/7p/8/8/4K3 w - - 0 1").unwrap();
        let mut moves = generate_legal_moves(&pos);
        order_moves(&pos, &mut moves, None, &[None, None]);

        let pxq = moves.iter().position(|mv| {
            // c5=file2,rank4 captures d6=file3,rank5
            mv.from == crate::types::Square::from_file_rank(2, 4)
            && mv.to == crate::types::Square::from_file_rank(3, 5)
        }).expect("PxQ move c5xd6 should exist");

        let qxp = moves.iter().position(|mv| {
            // h5=file7,rank4 captures h4=file7,rank3
            mv.from == crate::types::Square::from_file_rank(7, 4)
            && mv.to == crate::types::Square::from_file_rank(7, 3)
        }).expect("QxP move h5xh4 should exist");

        assert!(pxq < qxp, "PxQ (index {pxq}) should be ordered before QxP (index {qxp})");
    }

    #[test]
    fn mvv_lva_captures_before_quiet() {
        // All captures must appear before all quiet moves after ordering.
        let pos = Position::from_fen("4k3/8/3q4/2P4Q/7p/8/8/4K3 w - - 0 1").unwrap();
        let mut moves = generate_legal_moves(&pos);
        order_moves(&pos, &mut moves, None, &[None, None]);

        let last_cap = moves.iter().rposition(|mv| {
            pos.bbs.occupancy().contains(mv.to) || mv.kind == MoveKind::EnPassant
        });
        let first_quiet = moves.iter().position(|mv| {
            !pos.bbs.occupancy().contains(mv.to)
            && mv.kind != MoveKind::EnPassant
            && !matches!(mv.kind, MoveKind::Promotion(_))
        });

        if let (Some(cap_idx), Some(quiet_idx)) = (last_cap, first_quiet) {
            assert!(cap_idx < quiet_idx,
                "last capture (index {cap_idx}) should precede first quiet move (index {quiet_idx})");
        }
    }

    #[test]
    fn mvv_lva_tt_move_is_first() {
        // A TT move must be sorted before all captures.
        let pos = Position::from_fen("4k3/8/3q4/2P4Q/7p/8/8/4K3 w - - 0 1").unwrap();
        let moves_unsorted = generate_legal_moves(&pos);
        // Pick any move as the "TT move".
        let tt_move = moves_unsorted[moves_unsorted.len() - 1];
        let mut moves = moves_unsorted;
        order_moves(&pos, &mut moves, Some(tt_move), &[None, None]);
        assert_eq!(moves[0], tt_move, "TT move must be the first move tried");
    }

    // --- killer move tests ---

    #[test]
    fn killer_move_sorted_before_quiet() {
        // A killer move (quiet) must appear before other quiet moves but after captures.
        let pos = Position::from_fen("4k3/8/3q4/2P4Q/7p/8/8/4K3 w - - 0 1").unwrap();
        let mut moves = generate_legal_moves(&pos);
        // Pick a quiet move to designate as killer.
        let killer = moves.iter().find(|mv| {
            !pos.bbs.occupancy().contains(mv.to)
            && mv.kind != MoveKind::EnPassant
            && !matches!(mv.kind, MoveKind::Promotion(_))
        }).copied().expect("position should have quiet moves");

        order_moves(&pos, &mut moves, None, &[Some(killer), None]);

        // Find indices.
        let killer_idx = moves.iter().position(|mv| *mv == killer).unwrap();
        let last_capture_idx = moves.iter().rposition(|mv| {
            pos.bbs.occupancy().contains(mv.to) || mv.kind == MoveKind::EnPassant
        });
        let first_non_killer_quiet = moves.iter().position(|mv| {
            !pos.bbs.occupancy().contains(mv.to)
            && mv.kind != MoveKind::EnPassant
            && !matches!(mv.kind, MoveKind::Promotion(_))
            && *mv != killer
        });

        if let Some(cap_idx) = last_capture_idx {
            assert!(killer_idx > cap_idx, "killer should come after captures");
        }
        if let Some(quiet_idx) = first_non_killer_quiet {
            assert!(killer_idx < quiet_idx, "killer should come before other quiet moves");
        }
    }

    // --- Null move pruning tests ---

    #[test]
    fn null_move_does_not_corrupt_winning_eval() {
        // Queen + king vs king — clearly winning. Null move may prune but must not
        // corrupt the score. Kings are far apart so no king can be immediately captured.
        let pos = Position::from_fen("6k1/8/8/8/8/8/8/Q3K3 w - - 0 1").unwrap();
        let result = search(&pos, 4, &[], 0, 1, None, &mut tt());
        assert!(result.best_move.is_some());
        // Score should reflect the queen advantage (≥ ~800 cp); null move must not collapse it to 0.
        assert!(result.score > 500, "queen-up position should score > 500 cp, got {}", result.score);
    }

    #[test]
    fn null_move_skipped_in_kings_only_endgame() {
        // Only kings — has_pieces=false, so null move must not fire.
        // Evaluates near 0 regardless.
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        let result = search(&pos, 4, &[], 0, 1, None, &mut tt());
        assert!(result.best_move.is_some());
        assert!(result.score.abs() < 100, "kings-only should score near 0, got {}", result.score);
    }

    // --- Late move reduction tests ---

    #[test]
    fn lmr_does_not_miss_mate_in_one() {
        // Mate-in-1 must still be found at depth 4 even when LMR is active.
        let pos = Position::from_fen("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1").unwrap();
        let result = search(&pos, 4, &[], 0, 1, None, &mut tt());
        assert!(result.best_move.is_some());
        assert!(result.score >= MATE_SCORE - 10,
            "mate-in-1 must still be found with LMR active, got {}", result.score);
    }

    #[test]
    fn lmr_reduces_node_count_at_depth_5() {
        // LMR + null move together should keep depth-5 nodes well under 200k on the
        // starting position. Without either technique depth-5 is typically ~1M+ nodes.
        let pos = Position::starting_position();
        let result = search(&pos, 5, &[], 0, 1, None, &mut tt());
        assert!(result.best_move.is_some());
        assert!(result.nodes < 200_000,
            "depth-5 nodes should be < 200k with LMR+null move, got {}", result.nodes);
    }

    #[test]
    fn killer_not_in_legal_moves_does_not_crash() {
        // A killer from a sibling node may not be legal in this position — must not crash or
        // incorrectly place it first.
        let pos = Position::starting_position();
        let mut moves = generate_legal_moves(&pos);
        // Construct a bogus move that is definitely not in the legal list.
        let bogus = Move::normal(
            crate::types::Square::from_file_rank(4, 4),
            crate::types::Square::from_file_rank(4, 6),
        );
        // Should not panic and bogus move should not appear in ordered list.
        order_moves(&pos, &mut moves, None, &[Some(bogus), None]);
        assert!(!moves.contains(&bogus), "bogus killer must not be inserted into move list");
    }
}
