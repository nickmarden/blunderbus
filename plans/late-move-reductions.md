# Plan: Late Move Reductions (LMR)

## Goal

Moves searched late in the list (after good captures, killers, and the first
few quiet moves) are statistically unlikely to be best. Search them at reduced
depth first. If the reduced-depth search stays below alpha, skip the full-depth
re-search entirely. If it raises alpha, re-search at full depth to confirm.

LMR is one of the highest-impact search improvements available. It allows
the engine to effectively search deeper without proportionally more nodes.

---

## The rule

For the Nth quiet move at a node (N >= 3, typically), reduce depth by R:

```
if move_index >= 3 && depth >= 3 && !in_check && is_quiet(mv) {
    reduced_score = -negamax(child, depth - 1 - R, -alpha - 1, -alpha, ...)
    if reduced_score <= alpha {
        continue;  // reduced search confirmed it's bad — skip
    }
    // Otherwise fall through to full-depth search
}
full_score = -negamax(child, depth - 1, -beta, -alpha, ...)
```

The initial reduced search uses a zero window (`-alpha-1, -alpha`) because
we only want to know if it beats alpha, not the exact score.

---

## Reduction formula

Start with `R = 1` (reduce by 1 ply). A common improvement is to scale R
with depth and move index:

```rust
fn lmr_reduction(depth: u32, move_index: usize) -> u32 {
    if depth >= 3 && move_index >= 3 {
        1 + (depth as f32).ln() as u32 * (move_index as f32).ln() as u32 / 2
    } else {
        0
    }
}
```

For simplicity, start with flat `R = 1` and tune later.

---

## When NOT to reduce

Do not reduce:

- **Captures** — already ordered by MVV-LVA; likely to be important.
- **Killer moves** — already known to be good; don't reduce.
- **Moves that give check** — checking moves are often tactical.
- **When in check** — all evasions are critical.
- **Promotions** — always important.
- **At depth < 3** — too close to the horizon.
- **First 2-3 moves** — they've been ordered well and are likely good.

---

## Implementation in `negamax`

```rust
let mut move_index = 0;
for mv in &ordered_moves {
    let child = pos.make_move(*mv);
    let score = if move_index >= 3 && depth >= 3 && !in_check && is_quiet(mv) {
        let r = 1; // flat reduction for now
        let reduced = -negamax(&child, depth - 1 - r, -alpha - 1, -alpha, ...);
        if reduced > alpha {
            // Promising — re-search at full depth
            -negamax(&child, depth - 1, -beta, -alpha, ...)
        } else {
            reduced
        }
    } else {
        -negamax(&child, depth - 1, -beta, -alpha, ...)
    };
    move_index += 1;
    // alpha-beta update...
}
```

---

## Tests to write

1. Node count at depth 6 is lower with LMR than without.
2. Score at depth 5 for known tactical positions matches depth-5 without LMR
   (LMR should not change the result for clearly tactical positions).
3. LMR does not trigger on captures or killers.
4. LMR does not trigger when in check.

---

## Interaction with other techniques

LMR works best after good move ordering (MVV-LVA + killers). The late moves
really are bad, so reducing them is safe. Implement after both of those.

---

## Success criteria

- All existing tests pass.
- Perft counts unchanged.
- Effective search depth noticeably higher within the same node budget.
- Self-play quality at least as good as before at the same depth setting.
