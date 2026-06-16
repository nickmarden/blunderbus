mod types;
mod board;
mod bitboard;
mod position;
mod movegen;
mod eval;
mod search;
mod zobrist;
mod pgn;
mod options;
mod cli;
mod uci;

fn main() {
    let opts = options::CliOptions::from_args();
    if opts.uci {
        uci::run(&opts);
    } else {
        cli::run(opts);
    }
}
