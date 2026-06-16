# UCI Protocol Implementation Plan

## Summary

UCI (Universal Chess Interface) is the standard protocol that chess GUIs use to talk to chess
engines. Adding UCI support to blunderbus means it can plug into Arena, Cutechess, Lichess bot
API, and any other UCI-compatible tool — without changing a line of the search, move generation,
or evaluation code.

The changes are confined to three places:

1. A new `src/uci.rs` module that reads UCI commands from stdin and writes responses to stdout.
2. A `--uci` flag in `src/options.rs` that activates that mode.
3. A two-line change in `src/main.rs` to dispatch to `uci::run_uci()` instead of `cli::run()`.

The search engine, position representation, move parser (`parse_move`), and move formatter
(`move_label`) are already compatible with UCI — they already use long algebraic notation
(`e2e4`, `e7e8q`). That's the main reason this feature is straightforward.

---

## UCI Protocol Primer

UCI is a line-oriented text protocol over stdin/stdout. The GUI sends commands; the engine sends
responses. Both sides flush after every line. There is no handshake beyond `uci` / `uciok`.

### Commands the engine must handle

| Command | Meaning |
|---------|---------|
| `uci` | GUI is asking: "do you speak UCI?" Engine replies with `id` lines and `uciok`. |
| `isready` | GUI is asking: "are you ready to accept commands?" Engine replies `readyok`. |
| `ucinewgame` | A new game is starting; reset any per-game state. |
| `position startpos [moves e2e4 e7e5 ...]` | Set the board position. Optionally apply a move list. |
| `position fen <fen> [moves ...]` | Set position from FEN string, then optionally apply moves. |
| `go depth <n>` | Search to depth n and report `bestmove`. |
| `go movetime <ms>` | Search for at most this many milliseconds. (Defer — see Threading section.) |
| `go infinite` | Search until `stop` is received. (Defer — see Threading section.) |
| `stop` | Stop searching and report `bestmove` immediately. (Relevant only for threaded search.) |
| `quit` | Exit the process. |

Any unrecognized command should be silently ignored — the spec says so. Do not print an error.

### Outputs the engine must produce

| Output | When |
|--------|------|
| `id name Blunderbus` | In response to `uci`, before `uciok`. |
| `id author <your name>` | In response to `uci`, before `uciok`. |
| `uciok` | End of `uci` response. |
| `readyok` | In response to `isready`. |
| `info depth <d> score cp <cp> nodes <n> pv <move>` | During search, once per depth iteration. |
| `bestmove <move>` | When search is complete (or `stop` is received). |

The `score cp` value is in centipawns (100 = one pawn) from the engine's perspective (the side
to move). This matches what `SearchResult.score` already returns.

### Notation

UCI uses long algebraic notation: source square + destination square + optional promotion letter.
Examples: `e2e4`, `g1f3`, `e7e8q`. This is exactly what `move_label()` in `cli.rs` produces
and what `parse_move()` in `cli.rs` consumes. No translation layer needed.

---

## Module Design: `src/uci.rs`

### State

The UCI loop needs to track two things across commands:

```rust
pub fn run_uci() {
    let mut pos = Position::starting_position();
    let mut game_history: Vec<u64> = Vec::new();
    // ...
}
```

`game_history` holds the Zobrist hashes of every position that has been played (excluding the
current one). This is the same meaning as in `cli.rs` and feeds directly into
`search(pos, depth, &game_history, qdepth)` for repetition detection.

### Reading stdin

```rust
use std::io::{self, BufRead, Write};

let stdin = io::stdin();
for line in stdin.lock().lines() {
    let line = line.expect("stdin read error");
    let line = line.trim();
    // dispatch on line
}
```

In Rust, `io::stdin()` returns a handle to standard input. `.lock()` acquires an exclusive lock
on it and returns something that implements `BufRead` (a buffered reader — think `std::istream`
with line buffering). `.lines()` gives you an iterator where each `next()` call blocks until a
full line arrives. The `line` variable is a `Result<String, io::Error>`; `.expect()` unwraps it
or panics on error, which is fine here since a broken stdin means the GUI has gone away.

### Flushing stdout

After every UCI output line, you must flush stdout:

```rust
fn uci_send(msg: &str) {
    println!("{}", msg);
    io::stdout().flush().ok();
}
```

In Rust, `println!` writes to a buffered stdout. The buffer may not be flushed until it fills up
or the process exits. GUIs read line by line and will hang if you don't flush. `.flush().ok()`
calls flush and discards any error (there's nothing useful to do if stdout breaks).

### Command dispatch

```rust
fn dispatch(line: &str, pos: &mut Position, game_history: &mut Vec<u64>) -> bool {
    // returns false when "quit" is received
    if line == "uci" {
        uci_send("id name Blunderbus");
        uci_send("id author <your name>");
        uci_send("uciok");
    } else if line == "isready" {
        uci_send("readyok");
    } else if line == "ucinewgame" {
        *pos = Position::starting_position();
        game_history.clear();
    } else if line.starts_with("position") {
        handle_position(line, pos, game_history);
    } else if line.starts_with("go") {
        handle_go(line, pos, game_history);
    } else if line == "quit" {
        return false;
    }
    // unknown commands: silently ignore (required by spec)
    true
}
```

Note the `*pos = ...` syntax: `pos` is a `&mut Position` (a mutable reference, like a `Position*`
in C++). To replace the thing it points at, you dereference it with `*` and assign. If you wrote
`pos = Position::starting_position()` without the `*`, you'd be rebinding the local reference
variable, not changing the caller's value.

### Parsing the `position` command

The `position` command has two forms:

```
position startpos
position startpos moves e2e4 e7e5 g1f3
position fen rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1
position fen <fen> moves e7e5
```

Implementation:

```rust
fn handle_position(line: &str, pos: &mut Position, game_history: &mut Vec<u64>) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    // tokens[0] == "position"

    let mut idx = 1;
    if tokens.get(idx) == Some(&"startpos") {
        *pos = Position::starting_position();
        game_history.clear();
        idx += 1;
    } else if tokens.get(idx) == Some(&"fen") {
        idx += 1;
        // FEN is the next 6 tokens (placement, side, castling, ep, halfmove, fullmove)
        let fen_end = (idx + 6).min(tokens.len());
        let fen = tokens[idx..fen_end].join(" ");
        *pos = Position::from_fen(&fen).expect("invalid FEN");
        game_history.clear();
        idx = fen_end;
    }

    // Optional move list
    if tokens.get(idx) == Some(&"moves") {
        idx += 1;
        for mv_str in &tokens[idx..] {
            let legal = generate_legal_moves(pos);
            // Reuse parse_move logic (move it to a shared location, or inline it here)
            if let Some(mv) = find_move(mv_str, &legal) {
                game_history.push(pos.hash);
                *pos = pos.make_move(*mv);
            }
        }
    }
}
```

`tokens.get(idx)` returns `Option<&&str>` — an optional reference to an element. `Some(&"startpos")`
matches a reference to the string slice `"startpos"`. This is idiomatic Rust bounds-safe indexing;
it avoids a panic if the token list is shorter than expected.

The FEN parsing relies on the fact that a standard FEN has exactly 6 space-separated fields.
`tokens[idx..fen_end].join(" ")` reassembles them into a single string.

#### Refactoring parse_move

`parse_move` and the helper functions `parse_square`, `promotion_matches` currently live in
`cli.rs` as private functions. For UCI to reuse them, do one of:

- Move them to a new `src/notation.rs` module and make them `pub`.
- Move them to `src/movegen.rs` alongside the `Move` type (arguably the right home).
- Duplicate a simplified version in `uci.rs` (expedient but messy).

Recommended: move to `movegen.rs` under a `pub fn parse_move(...)` signature. The `Move` type
already lives there and the helpers are pure functions with no dependencies on `cli` state.

### Parsing the `go` command (depth only, for now)

```rust
fn handle_go(line: &str, pos: &Position, game_history: &[u64]) {
    let tokens: Vec<&str> = line.split_whitespace().collect();

    // Default depth if not specified
    let depth = tokens.windows(2)
        .find(|w| w[0] == "depth")
        .and_then(|w| w[1].parse::<u32>().ok())
        .unwrap_or(4);

    let qdepth = 6; // hardcoded for now; could become a UCI option later

    let result = search(pos, depth, game_history, qdepth);

    if let Some(mv) = result.best_move {
        // info line before bestmove
        uci_send(&format!(
            "info depth {} score cp {} nodes {} pv {}",
            result.depth,
            result.score,
            result.nodes,
            move_label(&mv),
        ));
        uci_send(&format!("bestmove {}", move_label(&mv)));
    } else {
        // No legal moves — stalemate or checkmate; send a null move
        uci_send("bestmove 0000");
    }
}
```

`move_label` is also currently in `cli.rs`. Move it to `movegen.rs` alongside `parse_move`.
It has no dependencies on `cli` state either.

`0000` is the conventional UCI null move — sent when the engine has no legal move. Some GUIs
also accept `(none)`. Use `0000` for compatibility.

### Full module skeleton

```rust
// src/uci.rs

use std::io::{self, BufRead, Write};

use crate::movegen::{generate_legal_moves, move_label, parse_move};
use crate::position::Position;
use crate::search::search;

pub fn run_uci() {
    let mut pos = Position::starting_position();
    let mut game_history: Vec<u64> = Vec::new();

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.expect("stdin read error");
        let line = line.trim().to_string();
        if !dispatch(&line, &mut pos, &mut game_history) {
            break;
        }
    }
}

fn uci_send(msg: &str) {
    println!("{}", msg);
    io::stdout().flush().ok();
}

fn dispatch(line: &str, pos: &mut Position, game_history: &mut Vec<u64>) -> bool {
    // ... as shown above
    true
}

fn handle_position(line: &str, pos: &mut Position, game_history: &mut Vec<u64>) {
    // ... as shown above
}

fn handle_go(line: &str, pos: &Position, game_history: &[u64]) {
    // ... as shown above
}
```

---

## Entry Point Changes

### `src/options.rs`

Add one field:

```rust
pub struct CliOptions {
    // ... existing fields ...
    pub uci: bool,
}
```

Parse it in `from_args()`:

```rust
let uci = args.iter().any(|a| a == "--uci");
// add `uci` to the struct literal at the end
```

### `src/main.rs`

Add the module declaration and branch in `main()`:

```rust
mod uci;   // add alongside the existing mod declarations

fn main() {
    let opts = options::CliOptions::from_args();
    if opts.uci {
        uci::run_uci();
    } else {
        cli::run(opts);
    }
}
```

UCI mode never calls into `cli::run()`, so none of the terminal rendering, ANSI codes, or
`println!("Blunderbus Chess Engine")` banner lines will appear. The UCI spec requires that
the engine produce no output except valid UCI protocol messages.

---

## Info Output

The `info` line reports search progress. The current `search()` function in `search.rs` uses
iterative deepening internally (depth 1 up to `max_depth`) but returns only the final result.
To emit one `info` line per depth, there are two options:

### Option A: Callback (recommended for now)

Add an optional callback parameter to `search()`:

```rust
pub fn search(
    pos: &Position,
    max_depth: u32,
    game_history: &[u64],
    qdepth: u32,
    on_depth: Option<&dyn Fn(&SearchResult)>,
) -> SearchResult
```

Inside the depth loop, after updating `result`:

```rust
if let Some(cb) = on_depth {
    cb(&result);
}
```

In UCI mode, pass a closure that calls `uci_send`:

```rust
let result = search(&pos, depth, game_history, qdepth, Some(&|r: &SearchResult| {
    if let Some(mv) = r.best_move {
        uci_send(&format!(
            "info depth {} score cp {} nodes {} pv {}",
            r.depth, r.score, r.nodes, move_label(&mv)
        ));
    }
}));
```

In CLI mode and all tests, pass `None` — zero behavioral change.

In Rust, `&dyn Fn(&SearchResult)` is a reference to a trait object — a pointer to any function
or closure that takes a `&SearchResult`. Think of it like a `std::function<void(const SearchResult&)>*`
in C++. Using `Option<&dyn Fn(...)>` means "optionally pass a callback"; `None` means no callback.

### Option B: Emit from `search.rs` directly (simpler but less flexible)

Add a `verbose: bool` parameter and `eprintln!` or `println!` from inside the loop. This is
fine for a learning project but mixes protocol concerns into the search module. Not recommended.

### Option C: Restructure search to expose depth iterations (more work, more power)

Expose `negamax_root` publicly and let the caller drive the depth loop. This gives the most
control but requires the most refactoring. Defer this until it's actually needed.

**Recommendation**: start with Option A (callback). It's a small, reversible change to `search.rs`
that keeps protocol logic out of the search module and requires no restructuring.

---

## Threading and Time Management

### Why this matters

When a GUI sends `go movetime 5000` or `go infinite`, it expects the engine to search in the
background and remain responsive to `stop`. This requires:

1. A search thread that runs independently.
2. A way to signal that thread to stop.
3. The main loop continuing to read stdin while search runs.

### The `Arc<AtomicBool>` pattern

The standard Rust approach:

```rust
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;

let stop_flag = Arc::new(AtomicBool::new(false));
let stop_for_thread = Arc::clone(&stop_flag);

let handle = thread::spawn(move || {
    // search runs here; periodically checks stop_for_thread.load(Ordering::Relaxed)
    search_with_stop(&pos, depth, &game_history, qdepth, stop_for_thread)
});

// main loop reads "stop", sets flag:
stop_flag.store(true, Ordering::Relaxed);
let result = handle.join().unwrap();
uci_send(&format!("bestmove {}", move_label(&result.best_move.unwrap())));
```

`Arc` is an atomically-reference-counted pointer — like `std::shared_ptr` in C++, but safe to
share across threads. `AtomicBool` is a thread-safe boolean that can be read and written without
a mutex. `Arc::clone` creates a second owner of the same flag; both owners point to the same
allocation. When the main thread sets `stop_flag.store(true, ...)`, the search thread sees it
the next time it calls `stop_for_thread.load(...)`.

`thread::spawn` takes a closure with the `move` keyword, which means the closure takes ownership
of any variables it captures (here, `pos`, `game_history`, `stop_for_thread`). This is required
because the thread may outlive the stack frame that created it.

### Recommendation: defer threading for now

Implementing `go depth N` without threading is completely sufficient to:

- Play games against blunderbus in Arena.
- Run automated matches with Cutechess.
- Submit to Lichess bot API (which drives engines via UCI with `go movetime`).

For a learning project, threading adds significant complexity — ownership, lifetimes across thread
boundaries, the `Send` trait, channel vs. shared-flag communication patterns. It deserves its own
session, not a footnote.

**Phase 1** (this plan): implement `go depth N` only. Silently ignore `movetime` and `infinite`
by responding with a fixed-depth search anyway. Acceptable for development and testing.

**Phase 2** (future): add a stop flag, thread the search, handle `go infinite` and `go movetime`.

---

## "UCI for the Player Side" (Future Extension)

The request mentions UCI for "the opponent side" — meaning blunderbus's CLI could use an external
UCI engine as its opponent instead of its own search. This is a separate feature.

How it would work:

1. Spawn a child process (e.g., Stockfish) with `std::process::Command::new("stockfish")`.
2. Write UCI commands to its stdin; read UCI responses from its stdout.
3. When it's the opponent's turn in `cli.rs`, send `position ... moves ...` and `go depth N`
   to the child process, wait for `bestmove`, parse the move, and apply it.

This is useful for testing blunderbus's play quality against a known engine without leaving the
blunderbus CLI. It requires process management and async I/O that is outside the scope of this
plan. Note it as a future feature in CLAUDE.md's TODO section when the time comes.

---

## Testing

### Manual testing with Arena (recommended first step)

Arena is a free, cross-platform chess GUI. To test:

1. Build a release binary: `cargo build --release`
2. In Arena: Engines > Install New Engine, point it at `target/release/blunderbus`, set type to UCI.
3. Start a game. Arena will send UCI commands and blunderbus should respond correctly.

Watch for:
- Does Arena see the engine respond to `uci` with `uciok`?
- Does it accept moves after `position` + `go depth 4`?
- Does the game proceed normally to checkmate or draw?

### Scripted stdin testing

For repeatable testing without a GUI, pipe a command sequence in:

```bash
echo -e "uci\nisready\nposition startpos\ngo depth 4\nquit" \
  | cargo run -- --uci
```

Expected output should include `uciok`, `readyok`, one `info` line, and one `bestmove` line.

You can write a shell script that pipes in a sequence of moves and asserts the output contains
the expected tokens. This is not as thorough as a proper UCI test harness but is fast and easy
to iterate with.

### Cutechess-cli (for match testing)

`cutechess-cli` is a command-line tool for running engine matches. Once UCI is working:

```bash
cutechess-cli \
  -engine cmd=./target/release/blunderbus arg=--uci name=Blunderbus \
  -engine cmd=./target/release/blunderbus arg=--uci name=Blunderbus2 \
  -each proto=uci tc=40/60 \
  -games 10 -pgnout games.pgn
```

This runs blunderbus against itself, which is a good sanity check that the UCI loop handles
game resets (`ucinewgame`) correctly between games.

---

## Implementation Order

1. Move `parse_move`, `parse_square`, `promotion_matches`, and `move_label` from `cli.rs` to
   `movegen.rs` as `pub` functions. Update `cli.rs` imports. Run `cargo test` — no test changes
   should be needed since the logic is identical.

2. Add `uci: bool` to `CliOptions` and parse `--uci` in `from_args()`.

3. Add `mod uci;` to `main.rs` and add the dispatch branch.

4. Write `src/uci.rs` with `run_uci()`, `dispatch()`, `handle_position()`, `handle_go()`,
   and `uci_send()`. Implement `go depth N` only; ignore other `go` variants.

5. Add the `on_depth` callback to `search()` in `search.rs`. Pass `None` everywhere except
   `handle_go()` in UCI mode.

6. Run `cargo build` and test manually with the `echo -e "uci\n..."` pipe trick.

7. Test with Arena.

8. Update CLAUDE.md: check off UCI in the Feature Status list, add threading to TODO.

---

## Future: Match Management Mode

The Unix-philosophy case for this: once blunderbus speaks UCI over stdin/stdout, it becomes a
building block that composes with anything. The natural next step is a built-in match coordinator
so you can watch `blunderbus vN` play `blunderbus vM` (or any UCI engine) without needing an
external GUI.

### What it would look like

```bash
blunderbus --match --white ./blunderbus-v1 --black ./blunderbus-v2 --games 10 --movetime 1000
blunderbus --match --white self --black ./stockfish --games 1 --depth 6
```

`self` means use blunderbus's own search directly (no subprocess). Any other value is treated as
a path to a UCI engine binary.

### Architecture

```
MatchEngine trait
  ├── LocalSearch   — calls search() directly; no subprocess
  └── UciProcess    — spawns a binary, pipes UCI over stdin/stdout

match_coordinator(white: Box<dyn MatchEngine>, black: Box<dyn MatchEngine>, games: u32)
  → plays N games (alternating colors each game), prints PGN + result table
```

`UciProcess` wraps `std::process::Command` with `stdin(Stdio::piped())` and
`stdout(Stdio::piped())`. Sending a move: write `position ... moves ...\ngo movetime N\n`,
read lines until one starts with `bestmove`, parse the move.

### Why this is worth doing eventually

- Regression testing: did the new evaluation function make it stronger?
- Watch two independent programs play each other — pure Unix joy.
- Generates self-play game records, which feeds directly into the LLM training data experiment
  described in CLAUDE.md (board state → next move prediction).
- Once you have match data you can feed it to `ordo` or `bayeselo` for ELO estimation.
- External tools like `c-chess-cli` and `cutechess-cli` do this already and can be used in the
  meantime — they just require both engines to speak UCI, which this PR delivers.

### Short-term workaround

Install `c-chess-cli` (single C file, trivial to build) and run:

```bash
c-chess-cli -engine cmd=./target/release/blunderbus args="--uci" \
            -engine cmd=./target/release/blunderbus-old args="--uci" \
            -each movetime=1 -games 10 -pgn out.pgn
```

This gives you match results today with zero extra code.
