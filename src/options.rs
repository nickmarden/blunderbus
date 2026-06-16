use std::time::{SystemTime, UNIX_EPOCH};

use crate::types::Color;

pub struct CliOptions {
    /// Print the static eval score after displaying the board.
    pub show_eval: bool,
    /// Before prompting the human, show the engine's suggested best move.
    pub show_hint: bool,
    /// Search depth for both the engine and hints (default 4).
    pub depth: u32,
    /// Render the board with Unicode pieces and ANSI colored squares.
    pub pretty: bool,
    /// Automatically accept the engine's suggested move for the human's turn.
    pub auto: bool,
    /// Which color the human plays.
    pub human_color: Color,
    /// Print the FEN string after every move.
    pub show_fen: bool,
    /// Print a PGN transcript of the game when it ends.
    pub show_pgn: bool,
    /// Skip the terminal clear between moves when --pretty is active.
    pub no_clear_screen: bool,
    /// Quiescence search depth cap (0 = disabled, default 6).
    pub qdepth: u32,
    /// How many top candidate moves to retain after search (default 3).
    pub candidates: usize,
    /// Engine strength 0-100. 100 = always best move; 0 = random among top candidates (default 100).
    pub strength: u8,
    /// Run in UCI protocol mode instead of the interactive CLI.
    pub uci: bool,
}

impl CliOptions {
    pub fn from_args() -> CliOptions {
        let args: Vec<String> = std::env::args().collect();

        let show_eval = args.iter().any(|a| a == "--eval"   || a == "-e");
        let show_hint = args.iter().any(|a| a == "--hint"   || a == "-h");
        let pretty    = args.iter().any(|a| a == "--pretty" || a == "-p");
        let auto      = args.iter().any(|a| a == "--auto"   || a == "-a");
        let show_fen        = args.iter().any(|a| a == "--fen"             || a == "-f");
        let show_pgn        = args.iter().any(|a| a == "--pgn");
        let no_clear_screen = args.iter().any(|a| a == "--no-clear-screen");

        let depth = args.windows(2)
            .find(|w| w[0] == "--depth" || w[0] == "-d")
            .and_then(|w| w[1].parse::<u32>().ok())
            .unwrap_or(4);

        let qdepth = args.windows(2)
            .find(|w| w[0] == "--qdepth")
            .and_then(|w| w[1].parse::<u32>().ok())
            .unwrap_or(6);

        let candidates = args.windows(2)
            .find(|w| w[0] == "--candidates" || w[0] == "-c")
            .and_then(|w| w[1].parse::<usize>().ok())
            .unwrap_or(3);

        let strength = args.windows(2)
            .find(|w| w[0] == "--strength")
            .and_then(|w| w[1].parse::<u8>().ok())
            .map(|s| s.min(100))
            .unwrap_or(100);

        let uci = args.iter().any(|a| a == "--uci");

        let human_color = if args.iter().any(|a| a == "--black") {
            Color::Black
        } else if args.iter().any(|a| a == "--random-color") {
            let ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos();
            if ns % 2 == 0 { Color::White } else { Color::Black }
        } else {
            Color::White
        };

        CliOptions { show_eval, show_hint, depth, qdepth, candidates, strength, uci, pretty, auto, human_color, show_fen, show_pgn, no_clear_screen }
    }
}
