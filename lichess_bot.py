#!/usr/bin/env python3
"""
Lichess bot wrapper for blunderbus.

Connects to the Lichess Bot API, streams incoming events, accepts challenges,
and manages one blunderbus UCI subprocess per active game.

Usage:
    python lichess_bot.py [--token TOKEN] [--depth N] [--max-games N]

Token is read from LICHESS_TOKEN env var or the .env file in this directory.
"""

import argparse
import json
import logging
import os
import queue
import subprocess
import sys
import threading
import time
from pathlib import Path

import requests

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

API_BASE = "https://lichess.org"
BLUNDERBUS_BIN = str(Path(__file__).parent / "target" / "release" / "blunderbus")

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(message)s",
    datefmt="%H:%M:%S",
)
log = logging.getLogger("bot")


# ---------------------------------------------------------------------------
# Token loading
# ---------------------------------------------------------------------------

def load_token(explicit_token: str | None) -> str:
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
    raise RuntimeError(
        "No Lichess token found. Set LICHESS_TOKEN env var or put it in .env"
    )


# ---------------------------------------------------------------------------
# Lichess API helpers
# ---------------------------------------------------------------------------

class LichessAPI:
    def __init__(self, token: str):
        self.session = requests.Session()
        self.session.headers.update({"Authorization": f"Bearer {token}"})

    def get(self, path: str, **kwargs) -> requests.Response:
        return self.session.get(f"{API_BASE}{path}", **kwargs)

    def post(self, path: str, **kwargs) -> requests.Response:
        return self.session.post(f"{API_BASE}{path}", **kwargs)

    def stream(self, path: str, **kwargs):
        """Yield parsed NDJSON lines from a streaming endpoint."""
        resp = self.session.get(
            f"{API_BASE}{path}", stream=True, timeout=60, **kwargs
        )
        resp.raise_for_status()
        for raw in resp.iter_lines():
            if raw:
                yield json.loads(raw)

    def account(self) -> dict:
        r = self.get("/api/account")
        r.raise_for_status()
        return r.json()

    def accept_challenge(self, challenge_id: str):
        r = self.post(f"/api/challenge/{challenge_id}/accept")
        if not r.ok:
            log.warning("Could not accept challenge %s: %s", challenge_id, r.text)

    def decline_challenge(self, challenge_id: str, reason: str = "generic"):
        self.post(
            f"/api/challenge/{challenge_id}/decline",
            data={"reason": reason},
        )

    def post_move(self, game_id: str, uci_move: str) -> bool:
        r = self.post(f"/api/bot/game/{game_id}/move/{uci_move}")
        if not r.ok:
            log.warning("Move %s rejected for game %s: %s", uci_move, game_id, r.text)
        return r.ok

    def resign(self, game_id: str):
        self.post(f"/api/bot/game/{game_id}/resign")

    def chat(self, game_id: str, room: str, text: str):
        self.post(
            f"/api/bot/game/{game_id}/chat",
            data={"room": room, "text": text},
        )


# ---------------------------------------------------------------------------
# UCI subprocess wrapper
# ---------------------------------------------------------------------------

class UCI:
    def __init__(self, depth: int):
        self.depth = depth
        self._proc = subprocess.Popen(
            [BLUNDERBUS_BIN, "--uci"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
        )
        self._lock = threading.Lock()
        self._send("uci")
        self._wait_for("uciok")
        self._send("isready")
        self._wait_for("readyok")

    def _send(self, cmd: str):
        self._proc.stdin.write(cmd + "\n")
        self._proc.stdin.flush()

    def _readline(self, timeout: float = 30.0) -> str | None:
        """Read one line with a deadline (uses a background thread)."""
        result: list[str] = []
        done = threading.Event()

        def reader():
            try:
                line = self._proc.stdout.readline()
                result.append(line.rstrip("\n"))
            except Exception:
                pass
            done.set()

        t = threading.Thread(target=reader, daemon=True)
        t.start()
        if not done.wait(timeout):
            return None
        return result[0] if result else None

    def _wait_for(self, keyword: str, timeout: float = 10.0) -> list[str]:
        lines = []
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            line = self._readline(timeout=max(0.1, deadline - time.monotonic()))
            if line is None:
                break
            lines.append(line)
            if keyword in line:
                break
        return lines

    def best_move(
        self,
        fen_or_moves: str,
        wtime: int | None = None,
        btime: int | None = None,
        winc: int = 0,
        binc: int = 0,
    ) -> str | None:
        with self._lock:
            self._send("ucinewgame")
            self._send(f"position {fen_or_moves}")

            if wtime is not None and btime is not None:
                go = f"go wtime {wtime} btime {btime} winc {winc} binc {binc}"
            else:
                go = f"go depth {self.depth}"
            self._send(go)

            lines = self._wait_for("bestmove", timeout=60.0)
            for line in lines:
                if line.startswith("bestmove"):
                    parts = line.split()
                    if len(parts) >= 2 and parts[1] != "(none)":
                        return parts[1]
            return None

    def quit(self):
        try:
            self._send("quit")
            self._proc.wait(timeout=3)
        except Exception:
            self._proc.kill()


# ---------------------------------------------------------------------------
# Per-game handler
# ---------------------------------------------------------------------------

class GameHandler:
    def __init__(self, api: LichessAPI, game_id: str, bot_color: str, depth: int):
        self.api = api
        self.game_id = game_id
        self.bot_color = bot_color  # "white" or "black"
        self.depth = depth
        self.uci = UCI(depth)
        self.moves: list[str] = []  # accumulated UCI moves from root
        self._done = False

    def _position_string(self) -> str:
        if self.moves:
            return "startpos moves " + " ".join(self.moves)
        return "startpos"

    def _is_my_turn(self) -> bool:
        # White moves on even-indexed plies (0, 2, 4…), Black on odd.
        if self.bot_color == "white":
            return len(self.moves) % 2 == 0
        return len(self.moves) % 2 == 1

    def handle_state(self, state: dict):
        """Process a gameState or gameFull event."""
        if state.get("type") == "gameFull":
            # Extract move list from the nested state field.
            inner = state.get("state", {})
            moves_str = inner.get("moves", "")
        else:
            # gameState event
            moves_str = state.get("moves", "")

        self.moves = moves_str.split() if moves_str.strip() else []

        status = (state.get("state") or state).get("status", "started")
        if status not in ("started", "created"):
            log.info("Game %s ended with status %s", self.game_id, status)
            self._done = True
            return

        if not self._is_my_turn():
            return

        wtime = (state.get("state") or state).get("wtime")
        btime = (state.get("state") or state).get("btime")
        winc  = (state.get("state") or state).get("winc", 0)
        binc  = (state.get("state") or state).get("binc", 0)

        log.info(
            "Game %s | thinking (move %d, %s to move) …",
            self.game_id,
            len(self.moves) + 1,
            self.bot_color,
        )
        move = self.uci.best_move(
            self._position_string(),
            wtime=wtime,
            btime=btime,
            winc=winc,
            binc=binc,
        )
        if move:
            log.info("Game %s | playing %s", self.game_id, move)
            self.api.post_move(self.game_id, move)
        else:
            log.error("Game %s | no move returned — resigning", self.game_id)
            self.api.resign(self.game_id)
            self._done = True

    def run(self):
        """Stream the game and react to state events; blocks until game over."""
        log.info("Game %s | streaming (I am %s)", self.game_id, self.bot_color)
        try:
            for event in self.api.stream(f"/api/bot/game/stream/{self.game_id}"):
                if self._done:
                    break
                etype = event.get("type", "")
                if etype in ("gameFull", "gameState"):
                    self.handle_state(event)
                elif etype == "chatLine":
                    pass  # ignore chat
                elif etype == "gameFinish":
                    log.info("Game %s | finished", self.game_id)
                    break
        except Exception as exc:
            log.error("Game %s | stream error: %s", self.game_id, exc)
        finally:
            self.uci.quit()
            log.info("Game %s | handler exiting", self.game_id)


# ---------------------------------------------------------------------------
# Main event loop
# ---------------------------------------------------------------------------

def should_accept(challenge: dict, max_games: int, active_count: int) -> tuple[bool, str]:
    """Return (accept, decline_reason)."""
    if active_count >= max_games:
        return False, "later"

    variant = challenge.get("variant", {}).get("key", "standard")
    if variant != "standard":
        return False, "variant"

    # Accept anything with a time control (or correspondence).
    return True, ""


def main():
    parser = argparse.ArgumentParser(description="Blunderbus Lichess bot")
    parser.add_argument("--token", help="Lichess OAuth token (overrides .env)")
    parser.add_argument("--depth", type=int, default=4, help="Search depth (default 4)")
    parser.add_argument("--max-games", type=int, default=4, help="Max concurrent games")
    args = parser.parse_args()

    token = load_token(args.token)
    api = LichessAPI(token)

    me = api.account()
    log.info("Logged in as %s (title: %s)", me["username"], me.get("title", "none"))

    if not Path(BLUNDERBUS_BIN).exists():
        log.error(
            "Binary not found at %s — run `cargo build --release` first",
            BLUNDERBUS_BIN,
        )
        sys.exit(1)

    active_games: dict[str, threading.Thread] = {}

    def start_game(game_id: str, color: str):
        handler = GameHandler(api, game_id, color, args.depth)
        t = threading.Thread(target=handler.run, name=f"game-{game_id}", daemon=True)
        t.start()
        active_games[game_id] = t

    def reap_finished():
        finished = [gid for gid, t in active_games.items() if not t.is_alive()]
        for gid in finished:
            del active_games[gid]

    log.info("Streaming events from Lichess…")
    while True:
        try:
            for event in api.stream("/api/stream/event"):
                reap_finished()
                etype = event.get("type", "")

                if etype == "challenge":
                    ch = event["challenge"]
                    cid = ch["id"]
                    accept, reason = should_accept(ch, args.max_games, len(active_games))
                    if accept:
                        log.info("Accepting challenge %s from %s", cid, ch["challenger"]["name"])
                        api.accept_challenge(cid)
                    else:
                        log.info("Declining challenge %s (reason: %s)", cid, reason)
                        api.decline_challenge(cid, reason)

                elif etype == "gameStart":
                    game = event["game"]
                    game_id = game["gameId"]
                    color = game.get("color", "white")
                    if game_id not in active_games:
                        start_game(game_id, color)

                elif etype == "gameFinish":
                    game_id = event.get("game", {}).get("gameId", "")
                    log.info("gameFinish event for %s", game_id)
                    reap_finished()

        except requests.exceptions.ConnectionError as exc:
            log.warning("Connection lost (%s), reconnecting in 5s…", exc)
            time.sleep(5)
        except requests.exceptions.HTTPError as exc:
            log.error("HTTP error: %s", exc)
            if exc.response is not None and exc.response.status_code == 401:
                log.error("Invalid token — check LICHESS_TOKEN")
                sys.exit(1)
            time.sleep(10)
        except KeyboardInterrupt:
            log.info("Interrupted — shutting down")
            break
        except Exception as exc:
            log.error("Unexpected error: %s", exc, exc_info=True)
            time.sleep(5)


if __name__ == "__main__":
    main()
