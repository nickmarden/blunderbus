# blunderbus

A chess engine written in Rust, built as a learning project to understand engine theory from the ground up. The long-term goal is to explore whether an LLM-style "next token" prediction model — where each token is a move and the input is a sequence of board states — can serve as a viable chess engine training approach. Getting there requires a working, well-understood engine first.

## Build

```
cargo build --release
```

Requires Rust (stable). Install via [rustup](https://rustup.rs) if needed.

## Play

```
cargo run -- --pretty --depth 4
```

Enter moves in coordinate notation: `e2e4`, `g1f3`, `e7e8q` (promotion). Press Enter to accept the suggested move when `--hint` is active. Type `quit` to exit.

## Options

| Flag | Default | Description |
|------|---------|-------------|
| `--depth N` / `-d N` | 4 | Search depth in plies |
| `--qdepth N` | 6 | Quiescence search depth cap (0 = disabled) |
| `--pretty` / `-p` | off | Unicode pieces with ANSI colored squares |
| `--eval` / `-e` | off | Show quiescence-stable eval score after each move |
| `--hint` / `-h` | off | Show the engine's suggested move before your prompt |
| `--black` | off | Play as Black (engine plays White) |
| `--random-color` | off | Randomly assign sides |
| `--auto` / `-a` | off | Engine plays both sides (useful for benchmarking) |
| `--pgn` | off | Print a PGN transcript when the game ends |
| `--fen` / `-f` | off | Print the FEN string after every move |
| `--no-clear-screen` | off | Suppress terminal clear in pretty mode |

### Examples

```bash
# Play at depth 5 with hints and eval display
cargo run -- --pretty --depth 5 --qdepth 6 --hint --eval

# Watch the engine play itself
cargo run -- --pretty --auto --depth 4

# Generate a PGN you can paste into Lichess analysis
cargo run -- --depth 4 --pgn

# Play a position from FEN (set via position.rs STARTING_FEN or future --fen-start flag)
```

## Architecture

| Module | Role |
|--------|------|
| `types.rs` | `Color`, `PieceKind`, `Piece`, `Square` |
| `board.rs` | 8x8 mailbox board (`[Option<Piece>; 64]`), FEN placement parsing |
| `position.rs` | Full game state: board + castling + en passant + clocks + Zobrist hash |
| `movegen.rs` | Pseudo-legal and legal move generation for all piece types; `perft` |
| `zobrist.rs` | Deterministic Zobrist hashing via xorshift64 PRNG (fixed seed) |
| `eval.rs` | Static evaluation: material + piece-square tables |
| `search.rs` | Negamax, alpha-beta pruning, iterative deepening, quiescence search |
| `pgn.rs` | Standard Algebraic Notation (SAN) conversion and PGN formatting |
| `options.rs` | CLI argument parsing |
| `cli.rs` | Interactive game loop and board rendering |

**Search**: negamax with alpha-beta pruning and iterative deepening. At each leaf node, quiescence search extends captures-only until the position is "quiet," preventing horizon-effect blunders. Threefold repetition (via Zobrist history) and the 50-move rule are enforced in both search and the game loop.

**Evaluation**: material values plus piece-square table bonuses. All six piece types have tables. Positive scores favor White.

## Status

Working and playable. Lichess analysis of games at `--depth 4 --qdepth 6` shows ~90%+ move accuracy. Known limitation: castling through check is not detected (pseudo-legal generator allows it; legal filter only catches landing in check).

Planned next: transposition table, better move ordering, UCI protocol, evaluation improvements (king safety, passed pawns), and eventually the LLM experiment.

## License

MIT
