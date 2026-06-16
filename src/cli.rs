use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::movegen::{generate_legal_moves, Move, MoveKind};
use crate::options::CliOptions;
use crate::pgn;
use crate::position::Position;
use crate::search::{quiescence_eval, search};
use crate::types::{Color, Piece, PieceKind, Square};

/// Run an interactive game loop. Human plays White by default; use --black to flip.
pub fn run(opts: CliOptions) {
    let mut pos = Position::starting_position();
    let mut game_history: Vec<u64> = Vec::new(); // position hashes before the current position
    let mut san_moves: Vec<String> = Vec::new();  // SAN transcript for PGN output
    let mut rng: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH).unwrap().subsec_nanos() as u64;

    println!("Blunderbus Chess Engine");
    println!("You are playing {}.", color_name(opts.human_color));
    println!("Enter moves in coordinate notation: e2e4, e7e8q (queen promotion), etc.");
    println!("Type 'quit' to exit.\n");

    // The labeled loop lets every exit path produce a PGN result string.
    // "&'static str" here means a string literal — it lives for the whole program.
    let game_result: &str = 'game: loop {
        // CLS here only for the human's turn — lets the human see the board before typing.
        // On the engine's turn we CLS inside the engine branch, right before its announcement,
        // so the announcement and the resulting board land on the same screen.
        let is_human_turn = pos.side_to_move == opts.human_color && !opts.auto;
        if opts.pretty && !opts.no_clear_screen && is_human_turn {
            print!("\x1b[H\x1b[2J");
            io::stdout().flush().ok();
        }
        render_position(&pos, opts.pretty);

        if pos.halfmove_clock >= 100 {
            println!("Draw by 50-move rule.");
            break "1/2-1/2";
        }

        // Threefold repetition: pos.hash appearing twice in game_history means
        // this is the third occurrence (game_history excludes the current position).
        if game_history.iter().filter(|&&h| h == pos.hash).count() >= 2 {
            println!("Draw by threefold repetition.");
            break "1/2-1/2";
        }

        let legal = generate_legal_moves(&pos);

        if legal.is_empty() {
            if pos.is_in_check(pos.side_to_move) {
                let winner = pos.side_to_move.opposite();
                println!("Checkmate! {} wins.", color_name(winner));
                break match winner {
                    Color::White => "1-0",
                    Color::Black => "0-1",
                };
            } else {
                println!("Stalemate! It's a draw.");
                break "1/2-1/2";
            }
        }

        if pos.side_to_move == opts.human_color {
            if opts.auto {
                // Auto mode: engine picks the human's move too.
                let result = search(&pos, opts.depth, &game_history, opts.qdepth, opts.candidates, None);
                let best = result.best_move.expect("legal moves exist but search returned none");
                let mv = select_move(&result.candidates, best, opts.strength, &mut rng);
                if opts.pretty && !opts.no_clear_screen {
                    print!("\x1b[H\x1b[2J");
                    io::stdout().flush().ok();
                }
                if opts.show_eval {
                    let white_score = if pos.side_to_move == Color::White { result.score } else { -result.score };
                    println!("Eval: {white_score:+} cp  (positive = White ahead)");
                }
                println!("Auto: {}", move_label(&mv));
                san_moves.push(pgn::move_to_san(&pos, &mv));
                game_history.push(pos.hash);
                pos = pos.make_move(&mv);
                if opts.show_fen { println!("FEN: {}", pos.to_fen()); }
            } else {
                // Run search once for both hint display and eval display (if either is active).
                let hint_result = if opts.show_hint || opts.show_eval {
                    if opts.show_hint {
                        print!("Hint: thinking...");
                        io::stdout().flush().ok();
                    }
                    let r = search(&pos, opts.depth, &game_history, opts.qdepth, opts.candidates, None);
                    if opts.show_hint {
                        print!("\r                  \r"); // erase "Hint: thinking..."
                        io::stdout().flush().ok();
                    }
                    Some(r)
                } else {
                    None
                };

                if opts.show_eval {
                    let white_score = match &hint_result {
                        Some(r) => if pos.side_to_move == Color::White { r.score } else { -r.score },
                        None    => quiescence_eval(&pos, opts.qdepth),
                    };
                    println!("Eval: {white_score:+} cp  (positive = White ahead)");
                }

                let hint = hint_result.and_then(|r| r.best_move);

                let prompt_str = match hint {
                    Some(mv) => format!("Your move [{}]: ", move_label(&mv)),
                    None     => "Your move: ".to_string(),
                };

                // The inner loop yields a Move. "break 'game" exits the outer loop early.
                let mv = loop {
                    let input = prompt(&prompt_str);
                    let trimmed = input.trim();
                    if trimmed.is_empty() {
                        if let Some(hm) = hint {
                            break hm;
                        }
                        continue;
                    }
                    if trimmed.eq_ignore_ascii_case("quit") {
                        println!("Goodbye.");
                        break 'game "*"; // exits outer loop with result "*" (abandoned)
                    }
                    match parse_move(trimmed, &legal) {
                        Ok(mv) => break mv,
                        Err(e) => println!("  {e}"),
                    }
                };
                san_moves.push(pgn::move_to_san(&pos, &mv));
                game_history.push(pos.hash);
                pos = pos.make_move(&mv);
                if opts.show_fen { println!("FEN: {}", pos.to_fen()); }
            }
        } else {
            // Engine turn
            print!("Engine thinking...");
            io::stdout().flush().ok();
            let result = search(&pos, opts.depth, &game_history, opts.qdepth, opts.candidates, None);
            println!();

            if opts.pretty && !opts.no_clear_screen {
                print!("\x1b[H\x1b[2J");
                io::stdout().flush().ok();
            }

            if opts.show_eval {
                let white_score = if pos.side_to_move == Color::White { result.score } else { -result.score };
                println!("Eval: {white_score:+} cp  (positive = White ahead)");
            }

            let engine_mv = result.best_move
                .map(|best| select_move(&result.candidates, best, opts.strength, &mut rng));
            match engine_mv {
                Some(mv) => {
                    let label = move_label(&mv);
                    println!("Engine plays: {label}  (score {}, {} nodes)", result.score, result.nodes);
                    san_moves.push(pgn::move_to_san(&pos, &mv));
                    game_history.push(pos.hash);
                    pos = pos.make_move(&mv);
                    if opts.show_fen { println!("FEN: {}", pos.to_fen()); }
                }
                None => {
                    println!("Engine has no moves.");
                    break "1/2-1/2";
                }
            }
        }

        println!();
    };

    if opts.show_pgn {
        let (white, black) = player_names(&opts);
        print!("\n{}", pgn::format_pgn(white, black, &san_moves, game_result));
    }
}

fn player_names(opts: &CliOptions) -> (&'static str, &'static str) {
    if opts.auto {
        ("Blunderbus", "Blunderbus")
    } else {
        match opts.human_color {
            Color::White => ("Human", "Blunderbus"),
            Color::Black => ("Blunderbus", "Human"),
        }
    }
}

fn render_position(pos: &Position, pretty: bool) {
    if !pretty {
        println!("{pos}");
        return;
    }

    const LIGHT_BG:       &str = "\x1b[48;5;238m"; // dark grey
    const DARK_BG:        &str = "\x1b[48;5;16m";  // black
    const WHITE_PIECE_FG: &str = "\x1b[97m";        // bright white
    const BLACK_PIECE_FG: &str = "\x1b[1m\x1b[38;5;250m"; // bold light grey
    const RESET:          &str = "\x1b[0m";

    for rank in (0..8u8).rev() {
        print!("{} ", rank + 1);
        for file in 0..8u8 {
            let sq = Square::from_file_rank(file, rank);
            let bg = if (file + rank) % 2 == 1 { LIGHT_BG } else { DARK_BG };
            let (glyph, fg) = match pos.board.get(sq) {
                Some(piece) => {
                    let fg = if piece.color == Color::White { WHITE_PIECE_FG } else { BLACK_PIECE_FG };
                    (unicode_piece(piece), fg)
                }
                None => (' ', ""),
            };
            print!("{bg}{fg} {glyph} {RESET}");
        }
        println!();
    }
    println!("   a  b  c  d  e  f  g  h");

    let stm = if pos.side_to_move == Color::White { "White" } else { "Black" };
    let ep = pos.en_passant.map_or("-".to_string(), |sq| sq.to_algebraic());
    println!("{stm} to move | Castling: {} | En passant: {ep} | Halfmove: {} | Move: {}",
        castling_str(pos), pos.halfmove_clock, pos.fullmove_number);
}

fn unicode_piece(piece: Piece) -> char {
    match (piece.color, piece.kind) {
        (Color::White, PieceKind::King)   => '♔',
        (Color::White, PieceKind::Queen)  => '♕',
        (Color::White, PieceKind::Rook)   => '♖',
        (Color::White, PieceKind::Bishop) => '♗',
        (Color::White, PieceKind::Knight) => '♘',
        (Color::White, PieceKind::Pawn)   => '♙',
        (Color::Black, PieceKind::King)   => '♚',
        (Color::Black, PieceKind::Queen)  => '♛',
        (Color::Black, PieceKind::Rook)   => '♜',
        (Color::Black, PieceKind::Bishop) => '♝',
        (Color::Black, PieceKind::Knight) => '♞',
        (Color::Black, PieceKind::Pawn)   => '♟',
    }
}

fn castling_str(pos: &Position) -> String {
    let mut s = String::new();
    if pos.castling.white_kingside  { s.push('K'); }
    if pos.castling.white_queenside { s.push('Q'); }
    if pos.castling.black_kingside  { s.push('k'); }
    if pos.castling.black_queenside { s.push('q'); }
    if s.is_empty() { "-".to_string() } else { s }
}

fn prompt(message: &str) -> String {
    print!("{message}");
    io::stdout().flush().ok();
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).ok();
    buf
}

/// Parse a coordinate move string like "e2e4" or "e7e8q" against the list of legal moves.
pub(crate) fn parse_move(input: &str, legal: &[Move]) -> Result<Move, String> {
    let bytes = input.as_bytes();

    if bytes.len() < 4 {
        return Err(format!("'{}' is too short; expected e.g. e2e4", input));
    }

    let from = parse_square(&bytes[0..2])
        .ok_or_else(|| format!("invalid source square '{}'", &input[0..2]))?;
    let to = parse_square(&bytes[2..4])
        .ok_or_else(|| format!("invalid destination square '{}'", &input[2..4]))?;

    // Optional promotion character
    let promo = if bytes.len() >= 5 {
        match bytes[4].to_ascii_lowercase() {
            b'q' => Some(PieceKind::Queen),
            b'r' => Some(PieceKind::Rook),
            b'b' => Some(PieceKind::Bishop),
            b'n' => Some(PieceKind::Knight),
            ch  => return Err(format!("unknown promotion piece '{}'", ch as char)),
        }
    } else {
        None
    };

    // Find the matching legal move
    let matched = legal.iter().find(|mv| {
        mv.from == from && mv.to == to && promotion_matches(mv, promo)
    });

    matched.copied().ok_or_else(|| format!("'{}' is not a legal move", input))
}

/// Check that a move's promotion kind matches what the user typed (or default to Queen).
fn promotion_matches(mv: &Move, requested: Option<PieceKind>) -> bool {
    match mv.kind {
        MoveKind::Promotion(kind) => match requested {
            Some(p) => p == kind,
            None    => kind == PieceKind::Queen, // auto-promote to queen when unspecified
        },
        _ => requested.is_none(),
    }
}

fn parse_square(bytes: &[u8]) -> Option<Square> {
    if bytes.len() < 2 {
        return None;
    }
    let file_ch = bytes[0].to_ascii_lowercase();
    let rank_ch = bytes[1];
    if !(b'a'..=b'h').contains(&file_ch) || !(b'1'..=b'8').contains(&rank_ch) {
        return None;
    }
    Some(Square::from_file_rank(file_ch - b'a', rank_ch - b'1'))
}

pub(crate) fn move_label(mv: &Move) -> String {
    let base = format!("{}{}", mv.from.to_algebraic(), mv.to.to_algebraic());
    match mv.kind {
        MoveKind::Promotion(kind) => {
            let suffix = match kind {
                PieceKind::Queen  => "q",
                PieceKind::Rook   => "r",
                PieceKind::Bishop => "b",
                PieceKind::Knight => "n",
                _ => "",
            };
            format!("{base}{suffix}")
        }
        _ => base,
    }
}

fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "White",
        Color::Black => "Black",
    }
}

/// Pick a move from `candidates` based on `strength` (0–100).
/// At 100 always returns `best`; at 0 picks uniformly at random from `candidates`;
/// at intermediate values picks randomly with probability `(100 - strength)%`.
fn select_move(candidates: &[(Move, i32)], best: Move, strength: u8, rng: &mut u64) -> Move {
    if strength >= 100 || candidates.len() <= 1 {
        return best;
    }
    let roll = lcg_next(rng) % 100;
    if roll < (100 - strength) as u64 {
        let idx = lcg_next(rng) as usize % candidates.len();
        candidates[idx].0
    } else {
        best
    }
}

/// Knuth multiplicative LCG — fast, no dependencies, good enough for strength randomisation.
fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    *state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movegen::MoveKind;

    fn dummy_move(from_idx: u8, to_idx: u8) -> Move {
        Move { from: Square::new(from_idx), to: Square::new(to_idx), kind: MoveKind::Normal }
    }

    #[test]
    fn select_move_strength_100_always_best() {
        let best = dummy_move(0, 8);
        let candidates = vec![(best, 100), (dummy_move(1, 9), 50), (dummy_move(2, 10), 10)];
        let mut rng = 12345u64;
        for _ in 0..20 {
            assert_eq!(select_move(&candidates, best, 100, &mut rng), best);
        }
    }

    #[test]
    fn select_move_single_candidate_always_best() {
        let best = dummy_move(0, 8);
        let candidates = vec![(best, 100)];
        let mut rng = 99u64;
        for _ in 0..20 {
            assert_eq!(select_move(&candidates, best, 0, &mut rng), best);
        }
    }

    #[test]
    fn select_move_strength_0_picks_from_pool() {
        let best = dummy_move(0, 8);
        let alt1 = dummy_move(1, 9);
        let alt2 = dummy_move(2, 10);
        let candidates = vec![(best, 100), (alt1, 50), (alt2, 10)];
        let mut rng = 1u64;
        // At strength 0, over many draws at least one non-best move should appear.
        let chosen: Vec<Move> = (0..100).map(|_| select_move(&candidates, best, 0, &mut rng)).collect();
        let non_best = chosen.iter().filter(|&&m| m != best).count();
        assert!(non_best > 0, "strength 0 should sometimes pick a non-best candidate");
    }
}
