# Strength Parameter Implementation Plan

## Summary

Add `--strength S` (0–100) to let the engine play weaker moves on purpose. Strength 100 is the
current behavior (always best move). Strength 0 picks uniformly at random from the top-N candidate
pool. Intermediate values blend the two probabilistically.

This feature is a thin layer on top of the top-N candidates feature (`plans/top-n-moves.md`). The
search itself is unchanged; strength affects only which move from the returned candidate pool gets
played.

---

## CLI Changes

### New field in `CliOptions` (`src/options.rs`)

```rust
pub struct CliOptions {
    // ... existing fields ...
    /// Engine strength 0–100 (100 = always best move, 0 = always random). Default 100.
    pub strength: u8,
}
```

### Parsing in `from_args()`

```rust
let strength = args.windows(2)
    .find(|w| w[0] == "--strength")
    .and_then(|w| w[1].parse::<u8>().ok())
    .map(|v| v.min(100))   // clamp silently; out-of-range → 100
    .unwrap_or(100);
```

Clamping to 100 is simpler than an error for a learning project. Add a note to the help text
listing the valid range.

### Help text addition

```
  --strength N      Engine strength 0-100 (default 100 = always best move)
```

---

## Move Selection Algorithm

### Inputs

```rust
fn select_move(candidates: &[(Move, i32)], strength: u8, rng: &mut impl Rng) -> Move
```

`candidates` is `Vec<(Move, i32)>` sorted best-first (index 0 = highest score), as provided by the
top-N candidates feature. `strength` is 0–100.

### Approach A: Threshold (recommended)

With probability `p = (100 - strength) / 100`, pick uniformly at random from the full candidate
list. Otherwise pick `candidates[0]`.

```
p_random = (100 - strength) / 100
if random_float() < p_random:
    return candidates[uniform_index(0..len)]
else:
    return candidates[0]
```

Examples:
- Strength 100: p_random = 0.0 — always best.
- Strength 50:  p_random = 0.5 — coin flip each move.
- Strength 0:   p_random = 1.0 — always random.

**Why this is recommended for a learning project:**

It is dead simple to understand, implement, and explain. The behavior maps directly to the
parameter: "strength 70 means 30% of moves are random." There is no temperature tuning, no
softmax, no floating-point score arithmetic. It also degrades gracefully when `candidates` has
only one entry: random selection over a 1-element list still returns that element.

### Approach B: Score-weighted softmax (not recommended for now)

Compute a probability distribution over candidates using a temperature T derived from strength,
then sample from that distribution.

```
T = temperature_from_strength(strength)   // large T = more uniform; T→0 = best only
weights[i] = exp(score[i] / T)
probabilities[i] = weights[i] / sum(weights)
return candidates[sample(probabilities)]
```

Pros: smoother gradient; even at intermediate strengths the second-best move is preferred over the
fifth-best. More "chess-realistic" randomness.

Cons: requires score normalization (scores can be large integers like ±900 cp); temperature
calibration is non-obvious; harder to explain to a learner; requires `exp` and float arithmetic.
Save this for a future enhancement once the basics work.

---

## RNG Strategy

Blunderbus currently has no randomness. Two options:

### Option A: Add `rand` crate (recommended)

```toml
# Cargo.toml
[dependencies]
rand = "0.8"
```

Usage:

```rust
use rand::Rng;
let mut rng = rand::thread_rng();
let idx = rng.gen_range(0..candidates.len());
```

`rand` is the de facto standard in the Rust ecosystem. It is small, well-maintained, and the usage
pattern is nearly identical to C++ `<random>`. `thread_rng()` is seeded from the OS and is fast.

### Option B: Roll a simple LCG

Implement a 64-bit linear congruential generator in a new file `src/rng.rs`. No new dependency,
but more code to write and maintain, worse statistical quality. Not worth it when `rand` is already
the ecosystem standard.

**Recommendation:** Add `rand = "0.8"`. The crate is a direct teaching moment (Rust's dependency
management via Cargo.toml, traits as interfaces via `impl Rng`). The compile-time cost is
negligible.

---

## Integration Points

### Where to apply (`src/cli.rs`)

Move selection happens after `search()` returns, before the move is played. No changes to
`src/search.rs`.

**Engine turn (single-player mode), current pattern:**

```rust
let result = search(&pos, opts.depth, &game_history, opts.qdepth);
let mv = result.best_move.expect("legal moves exist but search returned none");
```

**After this change:**

```rust
let result = search(&pos, opts.depth, &game_history, opts.qdepth);
let mv = select_move(&result.candidates, opts.strength, &mut rng);
```

`rng` is created once at the top of `run()` and passed through:

```rust
let mut rng = rand::thread_rng();
```

### Auto mode (`--auto`)

Strength applies to both sides. This is the simplest behavior and the right default: when you want
to watch two weaker engines play, you want the same policy for both. There is no separate
`--white-strength` / `--black-strength` at this stage. If per-side control is needed later, it can
be added as `--strength-white` / `--strength-black` that each override `--strength`.

### Hint display (`--hint`)

The hint should always show `result.candidates[0]` (the true best move), regardless of strength.
The hint is informational — showing the strength-degraded move would be confusing. In `cli.rs`,
the hint path calls `search()` separately and displays `hint_result.best_move`; that code path
does not go through `select_move()` and needs no change.

---

## Edge Cases

| Situation | Behavior |
|---|---|
| Only 1 legal move | `candidates` has 1 entry. Both best-pick and random-pick return it. No special case needed. |
| Strength 0, N=1 candidate | Same as above; uniform random over 1 element returns that element. |
| `candidates` is empty | Should not happen (caller should detect checkmate/stalemate first), but add a debug assert. |
| `--strength` not provided | Default 100; `select_move` always returns index 0. Zero overhead vs. current behavior. |
| `--strength 101` or higher | Clamp to 100 silently. |

---

## Code Sketch

### `src/options.rs` — add field and parsing

```rust
pub struct CliOptions {
    // existing...
    pub strength: u8,
}

// In from_args():
let strength = args.windows(2)
    .find(|w| w[0] == "--strength")
    .and_then(|w| w[1].parse::<u8>().ok())
    .map(|v| v.min(100))
    .unwrap_or(100);

// In struct literal:
CliOptions { ..., strength }
```

### `src/cli.rs` — `select_move` function and RNG wiring

```rust
use rand::Rng;

fn select_move(candidates: &[(Move, i32)], strength: u8, rng: &mut impl Rng) -> Move {
    debug_assert!(!candidates.is_empty(), "select_move called with empty candidate list");
    if strength == 100 || candidates.len() == 1 {
        return candidates[0].0;
    }
    let p_random = (100 - strength) as f64 / 100.0;
    if rng.gen::<f64>() < p_random {
        candidates[rng.gen_range(0..candidates.len())].0
    } else {
        candidates[0].0
    }
}

pub fn run(opts: CliOptions) {
    let mut rng = rand::thread_rng();
    // ...
    // Replace: let mv = result.best_move.expect(...);
    // With:
    let mv = select_move(&result.candidates, opts.strength, &mut rng);
}
```

### `Cargo.toml` — add dependency

```toml
[dependencies]
rand = "0.8"
```

---

## Implementation Order

1. Write `plans/top-n-moves.md` and implement `candidates: Vec<(Move, i32)>` in `SearchResult`.
2. Add `rand = "0.8"` to `Cargo.toml`.
3. Add `strength: u8` to `CliOptions` and parse `--strength`.
4. Implement `select_move` in `cli.rs`.
5. Wire `rng` through `run()` and replace `result.best_move` usages with `select_move(...)`.
6. Verify hint path is untouched.
7. Manual test: `--strength 0 --auto` should produce obviously randomized play; `--strength 100`
   should be indistinguishable from current behavior.
