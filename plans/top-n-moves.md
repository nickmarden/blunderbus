# Plan: Top-N Candidate Moves

Make the engine compute and retain the top N scored moves, primarily so the strength feature can
pick among them. Displaying them in the hint output is a secondary benefit.

---

## Summary

`negamax_root` already scores every legal move at the root iteration. The work to find the best
move and the work to rank all moves are the same loop — we just need to collect all (move, score)
pairs, sort them, and keep the top N. No extra searches, no significant overhead.

The key deliverable is `SearchResult.candidates: Vec<(Move, i32)>` — a sorted list of the top N
moves with scores that downstream code (strength selection, hint display) can consume. The display
in the CLI hint block is optional and layered on top; `candidates` is useful even if nothing
prints it.

---

## CLI Changes

### New flag: `--candidates N` / `-c N`

| Property | Value |
|----------|-------|
| Flag names | `--candidates`, `-c` |
| Type | `usize` |
| Default | `3` |
| Meaning | How many top moves to show when hint output is active |

The flag is independent of `--hint`. It sets the list length but has no effect unless `--hint` is
also passed (or `--auto`, see below). This keeps the two concerns separate: "should we show
candidates at all" stays with `--hint`; "how many" is `--candidates`.

### Changes to `CliOptions` in `src/options.rs`

Add one field and parse it the same way `depth` and `qdepth` are parsed:

```rust
pub struct CliOptions {
    // ... existing fields ...
    /// Number of candidate moves to show with --hint (default 3).
    pub candidates: usize,
}
```

Parsing (goes alongside the `qdepth` block):

```rust
let candidates = args.windows(2)
    .find(|w| w[0] == "--candidates" || w[0] == "-c")
    .and_then(|w| w[1].parse::<usize>().ok())
    .unwrap_or(3);
```

Add `candidates` to the final `CliOptions { ... }` struct literal.

Update the options table in `CLAUDE.md`:

| `--candidates N` / `-c N` | 3 | Number of top moves shown with `--hint` |

---

## SearchResult Changes

### New field in `SearchResult` (`src/search.rs`)

```rust
pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: i32,
    pub depth: u32,
    pub nodes: u64,
    /// Top-N moves with scores, sorted best-first (side-to-move perspective).
    /// Always populated at the final depth of iterative deepening.
    pub candidates: Vec<(Move, i32)>,
}
```

Initialize it empty in `search`:

```rust
let mut result = SearchResult {
    best_move: None, score: 0, depth: 0, nodes: 0,
    candidates: Vec::new(),
};
```

### Changes to `negamax_root`

Add a `n: usize` parameter so the caller controls list length. The function collects all scored
moves, sorts descending by score, truncates to N, and returns the list alongside the best move.

Change the return type from `(i32, Option<Move>)` to `(i32, Option<Move>, Vec<(Move, i32)>)`.

```rust
fn negamax_root(
    pos: &Position,
    depth: u32,
    nodes: &mut u64,
    history: &mut Vec<u64>,
    qdepth: u32,
    n: usize,
) -> (i32, Option<Move>, Vec<(Move, i32)>) {
    let mut moves = generate_legal_moves(pos);

    if moves.is_empty() {
        let score = if pos.is_in_check(pos.side_to_move) { -MATE_SCORE } else { 0 };
        return (score, None, Vec::new());
    }

    order_moves(pos, &mut moves);

    let mut scored: Vec<(Move, i32)> = Vec::with_capacity(moves.len());
    let mut alpha = -INFINITY;

    for mv in &moves {
        let after = pos.make_move(mv);
        history.push(after.hash);
        let score = -negamax(&after, depth - 1, -INFINITY, -alpha, 1, nodes, history, qdepth);
        history.pop();

        scored.push((*mv, score));

        if score > alpha {
            alpha = score;
        }
    }

    // Sort best-first. Use stable sort so move-order ties stay in generation order.
    scored.sort_by(|a, b| b.1.cmp(&a.1));

    let best_move = scored.first().map(|(mv, _)| *mv);
    scored.truncate(n);

    (alpha, best_move, scored)
}
```

Key point: alpha no longer gates what gets added to `scored`. Every move gets evaluated and
recorded. Alpha still drives the alpha-beta window passed into the recursive `negamax` calls, so
pruning efficiency is unchanged for the non-root nodes. The only cost is storing O(legal_moves)
pairs at the root, which is negligible.

### Calling site in `search`

```rust
let (score, mv, cands) = negamax_root(pos, depth, &mut nodes, &mut history, qdepth, n);
result.best_move = mv;
result.score = score;
result.depth = depth;
result.nodes += nodes;
result.candidates = cands; // overwritten each depth; final depth wins
```

`search` needs to accept `n: usize` and thread it through to `negamax_root`:

```rust
pub fn search(
    pos: &Position,
    max_depth: u32,
    game_history: &[u64],
    qdepth: u32,
    n: usize,
) -> SearchResult
```

All callers in `cli.rs` pass `opts.candidates`.

---

## Display Format

Print the candidate list immediately after the "Hint: thinking..." spinner is erased, before the
move prompt. Each line is indented two spaces for visual separation from the prompt.

Scores are displayed in pawn units (centipawns / 100.0), with a sign, one decimal place. The sign
makes it immediately clear whether the move is ahead or behind from the side-to-move's perspective.

```
  1. e4   +0.42
  2. Nf3  +0.38
  3. d4   +0.31
```

SAN notation comes from `pgn::move_to_san(&pos, &mv)`, which is already imported in `cli.rs`.

The move column should be left-padded with the rank number and right-padded to a fixed width (6
characters for the SAN string covers almost all cases; long disambiguated moves like `Nbxd2` are 5
chars). Use `{:<6}` format specifier.

### Display helper in `cli.rs`

```rust
fn print_candidates(pos: &Position, candidates: &[(Move, i32)]) {
    for (i, (mv, score)) in candidates.iter().enumerate() {
        let san = pgn::move_to_san(pos, mv);
        let pawns = *score as f32 / 100.0;
        println!("  {}. {:<6} {:+.2}", i + 1, san, pawns);
    }
}
```

Call this in the human-turn branch of `run`, right after erasing the spinner and before computing
the prompt string:

```rust
if opts.show_hint {
    print_candidates(&pos, &hint_result.candidates);
}
```

---

## Interaction with Existing Flags

### `--hint` (required for candidate display)

Candidates are only printed when `--hint` is active. Without `--hint`, `search` is still called
for `--eval`, but the results are used for the score display only. This avoids surprise output for
users who only want the eval number.

If someone passes `--candidates 5` without `--hint`, the `candidates` field is populated in the
`SearchResult` (because `search` always computes it), but nothing prints it. That is fine; the
computation cost is trivial.

### `--eval`

No change. The eval score is still taken from `result.score` and converted to White's perspective
as before.

### `--auto` mode

In `--auto`, the engine plays both sides. Currently it prints the chosen move after the search.
Showing candidates for each engine move in auto mode gives a useful window into what the engine
was considering.

Recommendation: print candidates in auto mode unconditionally (not gated on `--hint`), since
auto mode has no human interaction and there is no hint prompt to clutter. Keep it simple: if
`opts.auto` is true, always call `print_candidates` after the engine move announcement.

Alternative: gate it on `--hint` even in auto mode for consistency. This is slightly less useful
but simpler reasoning about the flag semantics.

Decision to make before implementing. The simpler path is to print candidates in auto mode
whenever `opts.candidates > 0` (i.e., always, since 0 is not a valid default). Passing
`--candidates 0` could suppress it, but that is an odd affordance. Recommend just always showing
candidates in auto mode and documenting it that way.

### `--hint` with empty move (Enter accepts hint)

Unchanged. The best move is still in `result.best_move` and drives the Enter-to-accept flow.

---

## Edge Cases

### Fewer than N legal moves

`scored.truncate(n)` is a no-op when `scored.len() <= n`. The list just shows however many moves
exist. No special handling needed.

### Mate in one / checkmate / stalemate

When `moves.is_empty()`, `negamax_root` returns early with an empty `Vec`. `SearchResult.candidates`
will be empty. `print_candidates` iterates zero times and prints nothing. Correct behavior.

When the engine finds a forced mate, the mate score (`100_000` or `-100_000` centipawns) will
display as `+1000.00` or `-1000.00` pawns. That is technically correct but visually odd. Options:

- Leave it as-is (simplest; users will understand).
- Detect `score.abs() >= MATE_SCORE / 2` and print `#` or `M` instead of a numeric score.

Recommendation: leave it as-is for now and note it as a future cosmetic improvement.

### `n = 0`

Passing `--candidates 0` would truncate to zero and print nothing. Since the default is 3 and the
flag is optional, this is not a real use case but it works correctly without a guard.

---

## Code Sketch

### `src/options.rs` diff summary

```rust
// Add to CliOptions struct:
pub candidates: usize,

// Add to from_args():
let candidates = args.windows(2)
    .find(|w| w[0] == "--candidates" || w[0] == "-c")
    .and_then(|w| w[1].parse::<usize>().ok())
    .unwrap_or(3);

// Add to final struct literal:
CliOptions { ..., candidates }
```

### `src/search.rs` diff summary

```rust
// SearchResult gains one field:
pub candidates: Vec<(Move, i32)>,

// search() signature change:
pub fn search(pos: &Position, max_depth: u32, game_history: &[u64], qdepth: u32, n: usize) -> SearchResult

// negamax_root signature change:
fn negamax_root(..., n: usize) -> (i32, Option<Move>, Vec<(Move, i32)>)

// Inside negamax_root: collect, sort_by, truncate (see full version above)

// Inside search loop, last depth iteration overwrites candidates:
result.candidates = cands;
```

### `src/cli.rs` diff summary

```rust
// New helper:
fn print_candidates(pos: &Position, candidates: &[(Move, i32)]) { ... }

// All search() call sites gain opts.candidates as the final argument.

// In human-turn hint block, after erasing spinner:
if opts.show_hint {
    print_candidates(&pos, &hint_result.candidates);
}

// In auto-mode engine-turn block, after announcing the move:
print_candidates(&pos, &result.candidates);
```

---

## Implementation Order

1. Add `candidates: usize` to `CliOptions` and parse `--candidates` / `-c`. Compile-check.
2. Add `candidates: Vec<(Move, i32)>` to `SearchResult`. Fix all construction sites (add
   `candidates: Vec::new()`). Compile-check.
3. Modify `negamax_root` to collect, sort, and truncate. Update its return type. Update the
   single call site in `search`. Compile-check.
4. Update `search` signature to accept `n: usize`. Update all call sites in `cli.rs`. Compile-check.
5. Add `print_candidates` helper in `cli.rs` and wire it into the hint and auto-mode paths.
6. Run `cargo test` to confirm no regressions. Manual smoke test with `--hint --candidates 5`.
