#!/usr/bin/env python3
"""
Fetch blunderbus's Lichess games and analyze each move against Stockfish.
Reports ACPL by game, phase, and piece type to guide engine tuning.

Usage:
    python lichess_accuracy.py
    python lichess_accuracy.py --max 50 --since 2024-01-01
    python lichess_accuracy.py --username somebot --movetime 500
"""

import argparse
import io
import os
import subprocess
import sys
import time
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

try:
    import chess
    import chess.pgn
except ImportError:
    print("Missing dependency: pip install chess", file=sys.stderr)
    sys.exit(1)

try:
    import requests
except ImportError:
    print("Missing dependency: pip install requests", file=sys.stderr)
    sys.exit(1)

API_BASE = "https://lichess.org"
DEFAULT_STOCKFISH = "/opt/homebrew/bin/stockfish"

# Phase detection weights matching eval.rs PHASE_WEIGHTS
_PHASE_WEIGHTS = {chess.KNIGHT: 1, chess.BISHOP: 1, chess.ROOK: 2, chess.QUEEN: 4}

# Cap per-move loss for ACPL averaging to prevent mate-score skew
ACPL_CAP = 1000

PIECE_NAMES = {
    chess.PAWN: "Pawn", chess.KNIGHT: "Knight", chess.BISHOP: "Bishop",
    chess.ROOK: "Rook", chess.QUEEN: "Queen", chess.KING: "King",
}


def load_token(explicit_token):
    if explicit_token:
        return explicit_token
    token = os.environ.get("LICHESS_TOKEN")
    if token:
        return token
    env_file = Path(__file__).parent / ".env"
    if env_file.exists():
        for line in env_file.read_text().splitlines():
            line = line.strip()
            if line.startswith("LICHESS_TOKEN="):
                return line.split("=", 1)[1].strip()
    return None


def date_to_ms(date_str):
    dt = datetime.strptime(date_str, "%Y-%m-%d").replace(tzinfo=timezone.utc)
    return int(dt.timestamp() * 1000)


def fetch_pgn_text(token, username, max_games, since_ms, until_ms):
    headers = {"Accept": "application/x-chess-pgn"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    params = {"max": max_games, "moves": "true", "tags": "true",
              "clocks": "false", "evals": "false"}
    if since_ms:
        params["since"] = since_ms
    if until_ms:
        params["until"] = until_ms
    resp = requests.get(f"{API_BASE}/api/games/user/{username}",
                        headers=headers, params=params, timeout=60)
    resp.raise_for_status()
    return resp.text


def board_phase(board):
    phase = sum(_PHASE_WEIGHTS.get(p.piece_type, 0) for p in board.piece_map().values())
    if phase > 16:
        return "opening"
    if phase > 8:
        return "middlegame"
    return "endgame"


class Stockfish:
    def __init__(self, path, threads, movetime_ms):
        self.movetime_ms = movetime_ms
        self._p = subprocess.Popen(
            [path], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True, bufsize=1,
        )
        self._send("uci")
        self._wait_for("uciok")
        self._send(f"setoption name Threads value {threads}")
        self._send("isready")
        self._wait_for("readyok")

    def _send(self, cmd):
        self._p.stdin.write(cmd + "\n")
        self._p.stdin.flush()

    def _wait_for(self, keyword, timeout=10.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            line = self._p.stdout.readline().rstrip()
            if keyword in line:
                return
        raise TimeoutError(f"Stockfish timed out waiting for {keyword!r}")

    def analyze(self, board):
        """Return (best_uci, cp) from side-to-move's perspective. cp is None on failure."""
        self._send(f"position fen {board.fen()}")
        self._send(f"go movetime {self.movetime_ms}")
        best, cp = "", None
        deadline = time.monotonic() + self.movetime_ms / 1000 + 5
        while time.monotonic() < deadline:
            line = self._p.stdout.readline().rstrip()
            if "score cp" in line:
                parts = line.split()
                try:
                    cp = int(parts[parts.index("cp") + 1])
                except (ValueError, IndexError):
                    pass
            elif "score mate" in line:
                parts = line.split()
                try:
                    m = int(parts[parts.index("mate") + 1])
                    cp = 10000 if m > 0 else -10000
                except (ValueError, IndexError):
                    pass
            if line.startswith("bestmove"):
                parts = line.split()
                best = parts[1] if len(parts) >= 2 and parts[1] != "(none)" else ""
                break
        return best, cp

    def quit(self):
        try:
            self._send("quit")
            self._p.wait(timeout=3)
        except Exception:
            self._p.kill()


def analyze_game(game, bb_username, sf):
    """
    Analyze blunderbus's moves only. Returns list of move records, or None
    if blunderbus is not a player in this game.

    cp_loss formula: eval_before (from blunderbus's POV) + eval_after_opp
    (from opponent's POV after blunderbus moves). If blunderbus played the
    best move these sum to ~0; a blunder makes eval_after_opp spike positive.
    """
    white = game.headers.get("White", "").lower()
    black = game.headers.get("Black", "").lower()
    bb_lower = bb_username.lower()
    if bb_lower == white:
        bb_color = chess.WHITE
    elif bb_lower == black:
        bb_color = chess.BLACK
    else:
        return None

    records = []
    board = game.board()
    for node in game.mainline():
        move = node.move
        if board.turn == bb_color:
            phase = board_phase(board)
            piece = board.piece_at(move.from_square)
            pname = PIECE_NAMES.get(piece.piece_type, "?") if piece else "?"
            label = f"{board.fullmove_number}{'w' if board.turn == chess.WHITE else 'b'}"
            fen_before = board.fen()

            best_uci, eval_before = sf.analyze(board)
            board.push(move)
            _, eval_after_opp = sf.analyze(board)

            if eval_before is not None and eval_after_opp is not None:
                cp_loss = max(0, eval_before + eval_after_opp)
            else:
                cp_loss = None

            records.append({
                "label": label,
                "phase": phase,
                "piece": pname,
                "played": move.uci(),
                "best": best_uci,
                "cp_loss": cp_loss,
                "fen": fen_before,
            })
        else:
            board.push(move)
    return records


def avg_capped(losses):
    capped = [min(x, ACPL_CAP) for x in losses if x is not None]
    return sum(capped) / len(capped) if capped else 0.0


def main():
    parser = argparse.ArgumentParser(
        description="Analyze blunderbus Lichess games vs Stockfish for engine tuning"
    )
    parser.add_argument("--username", default=None,
                        help="Lichess username (default: bot account from token)")
    parser.add_argument("--token", default=None, help="Lichess API token (overrides .env)")
    parser.add_argument("--max", type=int, default=20, help="Max games to fetch (default 20)")
    parser.add_argument("--since", default=None, help="Start date YYYY-MM-DD")
    parser.add_argument("--until", default=None, help="End date YYYY-MM-DD")
    parser.add_argument("--movetime", type=int, default=300,
                        help="Stockfish ms per position (default 300)")
    parser.add_argument("--threads", type=int, default=4,
                        help="Stockfish threads (default 4)")
    parser.add_argument("--stockfish", default=DEFAULT_STOCKFISH,
                        help=f"Stockfish binary (default {DEFAULT_STOCKFISH})")
    parser.add_argument("--top", type=int, default=20,
                        help="Worst moves to list (default 20)")
    parser.add_argument("--min-loss", type=int, default=50,
                        help="Min cp loss to include in worst-moves table (default 50)")
    args = parser.parse_args()

    token = load_token(args.token)
    username = args.username
    if not username:
        if not token:
            print("Need --username or a Lichess token in .env / LICHESS_TOKEN", file=sys.stderr)
            sys.exit(1)
        r = requests.get(f"{API_BASE}/api/account",
                         headers={"Authorization": f"Bearer {token}"})
        r.raise_for_status()
        username = r.json()["username"]

    print(f"Fetching up to {args.max} games for {username}...", flush=True)
    since_ms = date_to_ms(args.since) if args.since else None
    until_ms = date_to_ms(args.until) if args.until else None
    pgn_text = fetch_pgn_text(token, username, args.max, since_ms, until_ms)

    games = []
    pgn_io = io.StringIO(pgn_text)
    while True:
        g = chess.pgn.read_game(pgn_io)
        if g is None:
            break
        games.append(g)
    print(f"Loaded {len(games)} game(s).\n", flush=True)
    if not games:
        print("No games found.")
        return

    print(f"Starting Stockfish ({args.movetime}ms/position, {args.threads} threads)...", flush=True)
    try:
        sf = Stockfish(args.stockfish, args.threads, args.movetime)
    except FileNotFoundError:
        print(f"Stockfish not found at {args.stockfish!r}. Install: brew install stockfish")
        sys.exit(1)

    game_summaries = []
    all_records = []

    for i, game in enumerate(games):
        gid = game.headers.get("Site", "?").split("/")[-1]
        white = game.headers.get("White", "?")
        black = game.headers.get("Black", "?")
        result = game.headers.get("Result", "*")
        bb_color_str = "White" if white.lower() == username.lower() else "Black"
        opponent = black if bb_color_str == "White" else white

        print(f"  [{i+1}/{len(games)}] {gid}  vs {opponent} ({bb_color_str}) ...",
              end="  ", flush=True)
        records = analyze_game(game, username, sf)
        if records is None:
            print("skip")
            continue

        game_acpl = avg_capped([r["cp_loss"] for r in records])
        print(f"ACPL {game_acpl:.1f}  ({len(records)} moves)", flush=True)

        game_summaries.append({
            "id": gid, "opponent": opponent, "color": bb_color_str,
            "result": result, "moves": len(records), "acpl": game_acpl,
        })
        for r in records:
            r["game_id"] = gid
        all_records.extend(records)

    sf.quit()

    if not all_records:
        print("\nNo moves analyzed.")
        return

    sep = "=" * 72
    print()
    print(sep)
    print(f"BLUNDERBUS ACCURACY REPORT  |  {username}  |  {len(game_summaries)} game(s)")
    print(sep)
    print()

    # Per-game summary
    print(f"{'Game':<12} {'Opponent':<22} {'Color':<7} {'Result':<7} {'Moves':>5} {'ACPL':>6}")
    print("-" * 60)
    for s in game_summaries:
        print(f"{s['id']:<12} {s['opponent']:<22} {s['color']:<7} {s['result']:<7} "
              f"{s['moves']:>5} {s['acpl']:>6.1f}")
    overall = avg_capped([r["cp_loss"] for r in all_records])
    print()
    print(f"Overall ACPL: {overall:.1f}  ({len(all_records)} moves)")
    print()

    # ACPL by phase
    by_phase = defaultdict(list)
    for r in all_records:
        if r["cp_loss"] is not None:
            by_phase[r["phase"]].append(r["cp_loss"])
    print("ACPL by game phase:")
    for phase in ("opening", "middlegame", "endgame"):
        losses = by_phase.get(phase, [])
        print(f"  {phase:<12}  {avg_capped(losses):>6.1f}  ({len(losses)} moves)")
    print()

    # ACPL by piece type
    by_piece = defaultdict(list)
    for r in all_records:
        if r["cp_loss"] is not None:
            by_piece[r["piece"]].append(r["cp_loss"])
    piece_rows = sorted(
        ((p, avg_capped(ls), len(ls)) for p, ls in by_piece.items()),
        key=lambda x: -x[1],
    )
    print("ACPL by piece type (worst first):")
    for piece, avg, count in piece_rows:
        print(f"  {piece:<8}  {avg:>6.1f}  ({count} moves)")
    print()

    # Worst moves table
    worst = sorted(
        [r for r in all_records if r["cp_loss"] is not None and r["cp_loss"] >= args.min_loss],
        key=lambda r: -r["cp_loss"],
    )[:args.top]

    if worst:
        print(f"Top {len(worst)} worst moves (loss >= {args.min_loss}cp):")
        print()
        print(f"{'Game':<12} {'Ply':<6} {'Phase':<12} {'Piece':<8} {'Played':<8} {'Best':<8} {'Loss':>6}")
        print("-" * 64)
        for r in worst:
            print(f"{r['game_id']:<12} {r['label']:<6} {r['phase']:<12} {r['piece']:<8} "
                  f"{r['played']:<8} {r.get('best') or '?':<8} {r['cp_loss']:>6}")
        print()
        print("FENs for the top worst moves (paste into a board viewer or feed to blunderbus):")
        print()
        for r in worst[:10]:
            print(f"  # {r['game_id']} {r['label']}  played={r['played']}  "
                  f"best={r.get('best', '?')}  loss={r['cp_loss']}cp")
            print(f"  {r['fen']}")
            print()


if __name__ == "__main__":
    main()
