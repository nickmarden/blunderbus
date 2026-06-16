use std::io::{self, BufRead, Write};
use std::time::{Duration, Instant};

use crate::cli::{move_label, parse_move};
use crate::movegen::generate_legal_moves;
use crate::options::CliOptions;
use crate::position::Position;
use crate::search::search;
use crate::tt::TranspositionTable;
use crate::types::Color;

/// Run the UCI protocol loop, reading commands from stdin and writing responses to stdout.
///
/// Supported commands: uci, isready, ucinewgame, position, go depth N, stop, quit.
/// Threading / time management (go infinite, go movetime) is deferred.
pub fn run(opts: &CliOptions) {
    let mut pos = Position::starting_position();
    let mut game_history: Vec<u64> = Vec::new();
    let mut tt = TranspositionTable::new();

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        match tokens[0] {
            "uci" => {
                println!("id name Blunderbus");
                println!("id author Nick Marden");
                println!("uciok");
                io::stdout().flush().ok();
            }
            "isready" => {
                println!("readyok");
                io::stdout().flush().ok();
            }
            "ucinewgame" => {
                pos = Position::starting_position();
                game_history.clear();
                tt.clear();
            }
            "position" => {
                if let Some((p, h)) = parse_position(&tokens[1..]) {
                    pos = p;
                    game_history = h;
                }
            }
            "go" => {
                let go = parse_go(&tokens[1..], pos.side_to_move, opts.depth);
                let result = search(
                    &pos, go.max_depth, &game_history,
                    opts.qdepth, opts.candidates, go.deadline, &mut tt,
                );

                let pv = result.best_move
                    .map(|mv| move_label(&mv))
                    .unwrap_or_else(|| "0000".to_string());
                println!("info depth {} score cp {} nodes {} pv {}",
                    result.depth, result.score, result.nodes, pv);

                match result.best_move {
                    Some(mv) => println!("bestmove {}", move_label(&mv)),
                    None     => println!("bestmove 0000"),
                }
                io::stdout().flush().ok();
            }
            // stop with no threading support: nothing to stop, just ignore
            "stop" => {}
            "quit" => break,
            _ => {} // ignore unknown tokens
        }
    }
}

/// Parse the arguments after "position" — returns (final position, prior position hashes).
fn parse_position(tokens: &[&str]) -> Option<(Position, Vec<u64>)> {
    let mut idx = 0;

    let base_pos = if tokens.get(idx) == Some(&"startpos") {
        idx += 1;
        Position::starting_position()
    } else if tokens.get(idx) == Some(&"fen") {
        idx += 1;
        let fen_tokens: Vec<&str> = tokens[idx..]
            .iter()
            .take_while(|&&t| t != "moves")
            .copied()
            .collect();
        let fen = fen_tokens.join(" ");
        idx += fen_tokens.len();
        Position::from_fen(&fen).ok()?
    } else {
        return None;
    };

    let mut pos = base_pos;
    let mut history: Vec<u64> = Vec::new();

    if tokens.get(idx) == Some(&"moves") {
        idx += 1;
        for mv_str in &tokens[idx..] {
            let legal = generate_legal_moves(&pos);
            match parse_move(mv_str, &legal) {
                Ok(mv) => {
                    history.push(pos.hash);
                    pos = pos.make_move(&mv);
                }
                Err(_) => return None,
            }
        }
    }

    Some((pos, history))
}

struct GoParams {
    max_depth: u32,
    deadline: Option<Instant>,
}

/// Parse a `go` command's argument list into a depth cap and optional deadline.
///
/// Priority: `depth N` > `movetime N` > `wtime/btime` > default depth.
/// For `wtime/btime`, we spend roughly 1/30 of the remaining time (a simple but reasonable
/// formula; proper time management is a future improvement).
fn parse_go(tokens: &[&str], side: Color, default_depth: u32) -> GoParams {
    let find_u64 = |key: &str| -> Option<u64> {
        tokens.windows(2)
            .find(|w| w[0] == key)
            .and_then(|w| w[1].parse::<u64>().ok())
    };

    // `go depth N` — fixed depth, no time limit
    if let Some(d) = find_u64("depth") {
        return GoParams { max_depth: d as u32, deadline: None };
    }

    // `go movetime N` — spend exactly N ms
    if let Some(ms) = find_u64("movetime") {
        return GoParams {
            max_depth: default_depth,
            deadline: Some(Instant::now() + Duration::from_millis(ms)),
        };
    }

    // `go wtime X btime Y [winc Z binc Z]` — use 1/30 of remaining time
    let (time_key, inc_key) = match side {
        Color::White => ("wtime", "winc"),
        Color::Black => ("btime", "binc"),
    };
    if let Some(remaining_ms) = find_u64(time_key) {
        let inc_ms = find_u64(inc_key).unwrap_or(0);
        let budget_ms = remaining_ms / 30 + inc_ms / 2;
        let budget_ms = budget_ms.max(50); // never spend less than 50ms
        return GoParams {
            max_depth: default_depth,
            deadline: Some(Instant::now() + Duration::from_millis(budget_ms)),
        };
    }

    // Fallback: fixed depth, no deadline
    GoParams { max_depth: default_depth, deadline: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_position_startpos() {
        let tokens = vec!["startpos"];
        let (pos, history) = parse_position(&tokens).unwrap();
        assert_eq!(pos.to_fen(), Position::starting_position().to_fen());
        assert!(history.is_empty());
    }

    #[test]
    fn parse_position_startpos_with_moves() {
        let tokens = vec!["startpos", "moves", "e2e4", "e7e5"];
        let (pos, history) = parse_position(&tokens).unwrap();
        assert_eq!(history.len(), 2, "two prior positions pushed");
        // After e2e4 e7e5 it's White's turn, fullmove 2
        assert!(pos.to_fen().contains(" w "));
        assert!(pos.to_fen().contains(" 2"));
    }

    #[test]
    fn parse_position_fen() {
        let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
        let tokens = vec!["fen", "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR", "b", "KQkq", "e3", "0", "1"];
        let (pos, history) = parse_position(&tokens).unwrap();
        assert!(history.is_empty());
        assert_eq!(pos.to_fen(), fen);
    }

    #[test]
    fn parse_go_depth_sets_max_depth() {
        let p = parse_go(&["depth", "5"], Color::White, 4);
        assert_eq!(p.max_depth, 5);
        assert!(p.deadline.is_none());
    }

    #[test]
    fn parse_go_movetime_sets_deadline() {
        let p = parse_go(&["movetime", "500"], Color::White, 4);
        assert_eq!(p.max_depth, 4);
        assert!(p.deadline.is_some());
    }

    #[test]
    fn parse_go_wtime_sets_deadline() {
        let p = parse_go(&["wtime", "60000", "btime", "60000"], Color::White, 4);
        assert_eq!(p.max_depth, 4);
        assert!(p.deadline.is_some());
    }

    #[test]
    fn parse_go_default_uses_default_depth() {
        let p = parse_go(&[], Color::White, 4);
        assert_eq!(p.max_depth, 4);
        assert!(p.deadline.is_none());
    }

    #[test]
    fn parse_position_invalid_returns_none() {
        assert!(parse_position(&["garbage"]).is_none());
    }
}
