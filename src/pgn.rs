use crate::movegen::{generate_legal_moves, Move, MoveKind};
use crate::position::Position;
use crate::types::PieceKind;

/// Convert a move, given the position *before* it, to Standard Algebraic Notation.
///
/// SAN examples: "e4", "Nf3", "exd5", "O-O", "O-O-O", "e8=Q", "Rxe1+", "Qxf7#".
pub fn move_to_san(pos: &Position, mv: &Move) -> String {
    match mv.kind {
        MoveKind::CastleKingside => {
            let after = pos.make_move(mv);
            return format!("O-O{}", check_suffix(&after));
        }
        MoveKind::CastleQueenside => {
            let after = pos.make_move(mv);
            return format!("O-O-O{}", check_suffix(&after));
        }
        _ => {}
    }

    let piece = pos.board.get(mv.from).expect("move_to_san: no piece on from square");
    let is_capture = pos.board.get(mv.to).is_some() || mv.kind == MoveKind::EnPassant;
    let mut san = String::new();

    if piece.kind == PieceKind::Pawn {
        if is_capture {
            san.push((b'a' + mv.from.file()) as char);
            san.push('x');
        }
        san.push_str(&mv.to.to_algebraic());
        if let MoveKind::Promotion(kind) = mv.kind {
            san.push('=');
            san.push(piece_letter(kind));
        }
    } else {
        san.push(piece_letter(piece.kind));
        disambiguate(pos, mv, piece.kind, &mut san);
        if is_capture {
            san.push('x');
        }
        san.push_str(&mv.to.to_algebraic());
    }

    let after = pos.make_move(mv);
    san.push_str(check_suffix(&after));
    san
}

/// Format a complete PGN document from headers, a list of SAN moves, and the result string.
///
/// Result values: "1-0" (White wins), "0-1" (Black wins), "1/2-1/2" (draw), "*" (unfinished).
/// Line width is read from the COLUMNS environment variable, falling back to 80.
pub fn format_pgn(white: &str, black: &str, moves: &[String], result: &str) -> String {
    let width = std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(80);

    let mut pgn = String::new();

    pgn.push_str("[Event \"Blunderbus Game\"]\n");
    pgn.push_str("[Site \"Local\"]\n");
    pgn.push_str("[Date \"????.??.??\"]\n");
    pgn.push_str("[Round \"?\"]\n");
    pgn.push_str(&format!("[White \"{white}\"]\n"));
    pgn.push_str(&format!("[Black \"{black}\"]\n"));
    pgn.push_str(&format!("[Result \"{result}\"]\n"));
    pgn.push('\n');

    // Build the move list: "1. e4 e5 2. Nf3 Nc6 ... result", wrapped at `width` columns.
    let mut line = String::new();
    for (i, san) in moves.iter().enumerate() {
        if i % 2 == 0 {
            let num = format!("{}.", i / 2 + 1);
            push_token(&mut pgn, &mut line, &num, width);
        }
        push_token(&mut pgn, &mut line, san, width);
    }
    push_token(&mut pgn, &mut line, result, width);
    if !line.is_empty() {
        pgn.push_str(&line);
        pgn.push('\n');
    }

    pgn
}

// --- Internal helpers ---

/// Append any disambiguation characters needed before the destination square.
///
/// When two pieces of the same type can reach the same square, SAN requires a
/// disambiguator: the source file ("Ra" in "Rae1"), the source rank ("R1" in "R1e1"),
/// or both when three or more like pieces can all reach the destination.
fn disambiguate(pos: &Position, mv: &Move, kind: PieceKind, san: &mut String) {
    let ambiguous: Vec<_> = generate_legal_moves(pos)
        .into_iter()
        .filter(|m| {
            m.to == mv.to
                && m.from != mv.from
                && pos.board.get(m.from).map_or(false, |p| p.kind == kind)
        })
        .collect();

    if ambiguous.is_empty() {
        return;
    }

    // Is our source file unique among the competing pieces?
    let file_shared = ambiguous.iter().any(|m| m.from.file() == mv.from.file());
    let rank_shared = ambiguous.iter().any(|m| m.from.rank() == mv.from.rank());

    if !file_shared {
        san.push((b'a' + mv.from.file()) as char);
    } else if !rank_shared {
        san.push((b'1' + mv.from.rank()) as char);
    } else {
        // Both needed — requires 3+ pieces of the same type (possible after promotions).
        san.push((b'a' + mv.from.file()) as char);
        san.push((b'1' + mv.from.rank()) as char);
    }
}

/// Append `token` to `line`, flushing `line` to `pgn` first if it would overflow `width` chars.
fn push_token(pgn: &mut String, line: &mut String, token: &str, width: usize) {
    if line.is_empty() {
        line.push_str(token);
    } else if line.len() + 1 + token.len() <= width {
        line.push(' ');
        line.push_str(token);
    } else {
        pgn.push_str(line);
        pgn.push('\n');
        line.clear();
        line.push_str(token);
    }
}

fn check_suffix(pos: &Position) -> &'static str {
    if pos.is_in_check(pos.side_to_move) {
        if generate_legal_moves(pos).is_empty() { "#" } else { "+" }
    } else {
        ""
    }
}

fn piece_letter(kind: PieceKind) -> char {
    match kind {
        PieceKind::Knight => 'N',
        PieceKind::Bishop => 'B',
        PieceKind::Rook   => 'R',
        PieceKind::Queen  => 'Q',
        PieceKind::King   => 'K',
        PieceKind::Pawn   => panic!("pawn has no SAN piece letter"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movegen::MoveKind;
    use crate::types::{PieceKind, Square};

    fn sq(s: &str) -> Square {
        let b = s.as_bytes();
        Square::from_file_rank(b[0] - b'a', b[1] - b'1')
    }

    #[test]
    fn pawn_push_e4() {
        let pos = Position::starting_position();
        let mv = Move::normal(sq("e2"), sq("e4"));
        assert_eq!(move_to_san(&pos, &mv), "e4");
    }

    #[test]
    fn knight_f3() {
        let pos = Position::starting_position();
        let mv = Move::normal(sq("g1"), sq("f3"));
        assert_eq!(move_to_san(&pos, &mv), "Nf3");
    }

    #[test]
    fn pawn_capture_exd5() {
        // After 1.e4 d5, White can play exd5.
        let pos = Position::from_fen(
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
        ).unwrap();
        let mv = Move::normal(sq("e4"), sq("d5"));
        assert_eq!(move_to_san(&pos, &mv), "exd5");
    }

    #[test]
    fn kingside_castling() {
        let pos = Position::from_fen(
            "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1",
        ).unwrap();
        let mv = Move { from: sq("e1"), to: sq("g1"), kind: MoveKind::CastleKingside };
        assert_eq!(move_to_san(&pos, &mv), "O-O");
    }

    #[test]
    fn queenside_castling() {
        let pos = Position::from_fen(
            "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1",
        ).unwrap();
        let mv = Move { from: sq("e1"), to: sq("c1"), kind: MoveKind::CastleQueenside };
        assert_eq!(move_to_san(&pos, &mv), "O-O-O");
    }

    #[test]
    fn promotion_to_queen() {
        // White pawn on e7, enemy king on h1, own king on e1. Push e8=Q.
        let pos = Position::from_fen("8/4P3/8/8/8/8/8/4K2k w - - 0 1").unwrap();
        let mv = Move { from: sq("e7"), to: sq("e8"), kind: MoveKind::Promotion(PieceKind::Queen) };
        let san = move_to_san(&pos, &mv);
        assert!(san.starts_with("e8=Q"), "expected 'e8=Q...', got '{san}'");
    }

    #[test]
    fn checkmate_suffix() {
        // Ra1-a8 is checkmate: Black king on g8 is trapped by pawns on f7/g7/h7.
        let pos = Position::from_fen("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1").unwrap();
        let mv = Move::normal(sq("a1"), sq("a8"));
        assert_eq!(move_to_san(&pos, &mv), "Ra8#");
    }

    #[test]
    fn format_pgn_structure() {
        let moves = vec![
            "e4".to_string(), "e5".to_string(),
            "Nf3".to_string(), "Nc6".to_string(),
        ];
        let pgn = format_pgn("Human", "Blunderbus", &moves, "1-0");
        assert!(pgn.contains("[White \"Human\"]"));
        assert!(pgn.contains("[Black \"Blunderbus\"]"));
        assert!(pgn.contains("[Result \"1-0\"]"));
        assert!(pgn.contains("1. e4 e5 2. Nf3 Nc6"));
        assert!(pgn.ends_with("1-0\n"));
    }

    #[test]
    fn format_pgn_result_at_end() {
        let moves = vec!["e4".to_string()];
        let pgn = format_pgn("A", "B", &moves, "*");
        assert!(pgn.ends_with("1. e4 *\n") || pgn.ends_with("*\n"),
            "result token should appear at end: {pgn:?}");
    }
}
