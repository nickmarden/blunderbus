mod types;
mod board;
mod position;
mod movegen;
mod eval;
mod search;
mod zobrist;
mod pgn;
mod options;
mod cli;

fn main() {
    cli::run(options::CliOptions::from_args());
}
