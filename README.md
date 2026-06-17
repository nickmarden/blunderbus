# blunderbus

A chess engine written in Rust, built as a learning project to understand engine theory from the ground up. The long-term research question: can an LLM-style "next token" prediction model — where the output token is a move and the input is the current board state — serve as a viable chess engine training approach? Building blunderbus is about developing deep intuition for how engines work before tackling that experiment; the experiment itself could use Stockfish or any other engine for data generation.

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

## Lichess bot

blunderbus can connect to [Lichess](https://lichess.org) as a bot via `lichess_bot.py`.

### Prerequisites

1. A Lichess account [upgraded to BOT status](https://lichess.org/api#tag/Bot/operation/botAccountUpgrade):
   ```bash
   curl -X POST https://lichess.org/api/bot/account/upgrade \
     -H "Authorization: Bearer YOUR_TOKEN"
   ```
   This is irreversible — the account can only be used as a bot afterward.

2. An API token with the `bot:play` scope. Store it in a `.env` file in the project root:
   ```
   LICHESS_TOKEN=lip_xxxxxxxxxxxx
   ```

3. A release build:
   ```bash
   cargo build --release
   ```

4. Python dependencies (first time only):
   ```bash
   python3 -m venv .venv && .venv/bin/pip install -r requirements.txt
   ```

### Manual mode

Accept incoming challenges only — the bot sits and waits:

```bash
.venv/bin/python lichess_bot.py --depth 4
```

| Flag | Default | Description |
|------|---------|-------------|
| `--depth N` | 4 | Search depth in plies |
| `--candidates N` | 1 | Top-N moves for strength randomization |
| `--strength N` | 100 | 0–100: % chance to pick best move vs random from top-N |
| `--max-games N` | 4 | Max concurrent games |
| `--instance-key KEY` | off | Isolates this instance when running multiple copies (see below) |
| `--debug` | off | Log every UCI line exchanged with the engine |

### Auto-challenge mode

Automatically challenge other bots whenever a slot is free:

```bash
.venv/bin/python lichess_bot.py --auto-challenge --clock-limit 180 --clock-increment 2
```

The bot fetches up to 50 online bots every 15 seconds, picks one at random that meets the ELO filter (if set), and issues a challenge. Declined or expired challenges free the slot immediately.

| Flag | Default | Description |
|------|---------|-------------|
| `--auto-challenge` | off | Enable automatic outgoing challenges |
| `--clock-limit N` | 180 | Base time in seconds (180 = 3 min) |
| `--clock-increment N` | 2 | Increment per move in seconds |
| `--rated` / `--no-rated` | rated | Whether challenges are rated |
| `--min-elo N` | off | Only challenge bots rated at least N at this time control |
| `--max-elo N` | off | Only challenge bots rated at most N at this time control |

Examples:

```bash
# 3+2 blitz, target bots rated 1800–2200
.venv/bin/python lichess_bot.py --auto-challenge \
  --clock-limit 180 --clock-increment 2 \
  --min-elo 1800 --max-elo 2200

# 5+3 blitz, unrated, any strength
.venv/bin/python lichess_bot.py --auto-challenge \
  --clock-limit 300 --clock-increment 3 --no-rated
```

### Multiple instances

To run two instances with different settings (e.g., different depths) without them competing for the same games, give each a unique `--instance-key`:

```bash
# Terminal 1
.venv/bin/python lichess_bot.py --instance-key A --depth 4 --auto-challenge ...

# Terminal 2
.venv/bin/python lichess_bot.py --instance-key B --depth 8 --auto-challenge ...
```

Each instance claims its games via a lock file in `/tmp`. If you restart an instance with the same key, it picks up its in-progress games automatically.

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
