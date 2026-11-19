#!/usr/bin/env python3
"""Serwer podglądu i sterowania ArndtAi (warstwa UI, nie agent).

Serwuje `app/web/` oraz `.agent/sessions/` z workspace i pozwala uruchomić
zadanie z przeglądarki: `POST /api/run`. Sam agent zostaje w Rust — ten plik
nie zna pętli, tylko uruchamia binarkę jako proces potomny.

    python3 app/web/serve.py --workspace /tmp/ws --port 8766

Zasady:
- bind wyłącznie 127.0.0.1,
- jedno zadanie naraz (409 przy drugim),
- żadnych ścieżek spoza katalogu sesji (whitelist z listingu),
- treść zadania jest treścią, nie ścieżką.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse, parse_qs

WEB_DIR = Path(__file__).resolve().parent
SAFE_ID = re.compile(r"^[A-Za-z0-9._-]+$")
MIME = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".json": "application/json; charset=utf-8",
    ".png": "image/png",
    ".md": "text/markdown; charset=utf-8",
}


class Run:
    """Stan jednego biegu: proces, sesja, wynik."""

    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.proc: subprocess.Popen | None = None
        self.session: str | None = None
        self.started = 0.0
        self.code: int | None = None
        self.error: str | None = None

    def busy(self) -> bool:
        return self.proc is not None and self.proc.poll() is None

    def state(self) -> dict:
        code = self.code
        if self.proc is not None and code is None:
            code = self.proc.poll()
        return {
            "running": self.busy(),
            "session": self.session,
            "started": self.started,
            "code": code,
            "error": self.error,
        }


class Handler(BaseHTTPRequestHandler):
    server_version = "ArndtAiUI/0.3"
    workspace: Path
    binary: str
    run = Run()

    # ── pomoc ────────────────────────────────────────────────────────────
    def log_message(self, fmt: str, *args) -> None:  # cisza w konsoli
        if os.environ.get("ARNDTAI_UI_VERBOSE"):
            super().log_message(fmt, *args)

    def send_json(self, code: int, payload) -> None:
        body = json.dumps(payload, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def sessions_dir(self) -> Path:
        return self.workspace / ".agent" / "sessions"

    def session_files(self) -> dict[str, Path]:
        out: dict[str, Path] = {}
        d = self.sessions_dir()
        if not d.is_dir():
            return out
        for p in sorted(d.glob("*.jsonl")):
            stem = p.name[: -len(".jsonl")]
            if SAFE_ID.match(stem):
                out[stem] = p
        return out

    @staticmethod
    def summarize(path: Path) -> dict:
        task = ""
        status = "brak"
        last_at = 0
        try:
            with path.open("r", encoding="utf-8", errors="replace") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        ev = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    kind = ev.get("kind")
                    data = ev.get("data") or {}
                    if kind == "task" and not task:
                        task = str(data.get("text", ""))
                    if kind == "summary":
                        status = str(data.get("status", status))
                    last_at = max(last_at, int(ev.get("at") or 0))
            stat = path.stat()
        except OSError:
            stat = None
        return {
            "task": task,
            "status": status,
            "lastAt": last_at,
            "size": stat.st_size if stat else 0,
            "mtime": int(stat.st_mtime) if stat else 0,
        }

    # ── routing ──────────────────────────────────────────────────────────
    def do_GET(self) -> None:  # noqa: N802
        url = urlparse(self.path)
        if url.path == "/api/sessions":
            items = []
            for sid, path in self.session_files().items():
                items.append({"id": sid, **self.summarize(path)})
            items.sort(key=lambda x: (x["lastAt"], x["mtime"]), reverse=True)
            return self.send_json(200, {"sessions": items})

        if url.path == "/api/status":
            return self.send_json(200, self.run.state())

        if url.path.startswith("/api/sessions/"):
            sid = url.path[len("/api/sessions/"):]
            if not SAFE_ID.match(sid):
                return self.send_json(400, {"error": "zła nazwa sesji"})
            path = self.session_files().get(sid)
            if path is None:
                return self.send_json(404, {"error": "nie ma takiej sesji"})
            since = 0
            qs = parse_qs(url.query)
            if "since" in qs:
                try:
                    since = max(0, int(qs["since"][0]))
                except ValueError:
                    return self.send_json(400, {"error": "since musi być liczbą"})
            data = path.read_bytes()[since:]
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("X-Offset", str(since))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(data)
            return

        # Ta sama ścieżka, której używa app.js przy `python3 -m http.server`:
        # `/.agent/sessions/<id>.jsonl` z obsługą Range (tail inkrementalny).
        prefix = "/.agent/sessions/"
        if url.path == prefix:
            rows = "".join(
                f'<li><a href="{sid}.jsonl">{sid}.jsonl</a></li>'
                for sid in self.session_files()
            )
            body = f"<html><body><ul>{rows}</ul></body></html>".encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return

        if url.path.startswith(prefix):
            name = url.path[len(prefix):]
            if name.endswith(".jsonl"):
                name = name[: -len(".jsonl")]
            if not SAFE_ID.match(name):
                return self.send_json(400, {"error": "zła nazwa sesji"})
            path = self.session_files().get(name)
            if path is None:
                return self.send_json(404, {"error": "nie ma takiej sesji"})
            data = path.read_bytes()
            start = 0
            rng = self.headers.get("Range") or ""
            if rng.startswith("bytes="):
                try:
                    start = max(0, int(rng[6:].split("-")[0] or 0))
                except ValueError:
                    start = 0
            chunk = data[start:]
            code = 206 if (rng or start) and start < len(data) else 200
            self.send_response(code)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(chunk)))
            self.send_header("Content-Range", f"bytes {start}-{len(data) - 1}/{len(data)}")
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(chunk)
            return

        return self.serve_static(url.path)

    def serve_static(self, path: str) -> None:
        rel = path
        if rel in ("", "/"):
            rel = "/index.html"
        if rel.startswith("/web/"):
            rel = rel[len("/web"):]
        if rel == "/app/web/" or rel.startswith("/app/web/"):
            rel = rel[len("/app/web"):]
        if rel in ("", "/"):
            rel = "/index.html"
        target = (WEB_DIR / rel.lstrip("/")).resolve()
        try:
            target.relative_to(WEB_DIR)
        except ValueError:
            return self.send_json(403, {"error": "poza katalogiem UI"})
        if not target.is_file():
            return self.send_json(404, {"error": "nie ma pliku"})
        body = target.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", MIME.get(target.suffix, "application/octet-stream"))
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self) -> None:  # noqa: N802
        url = urlparse(self.path)
        if url.path == "/api/stop":
            with self.run.lock:
                if self.run.proc is not None and self.run.proc.poll() is None:
                    self.run.proc.terminate()
            return self.send_json(200, {"stopped": True})

        if url.path != "/api/run":
            return self.send_json(404, {"error": "nie ma takiego endpointu"})

        ctype = (self.headers.get("Content-Type") or "").split(";")[0].strip()
        if ctype != "application/json":
            return self.send_json(415, {"error": "wymagany Content-Type: application/json"})
        length = int(self.headers.get("Content-Length") or 0)
        if length <= 0 or length > 64 * 1024:
            return self.send_json(400, {"error": "złe ciało żądania"})
        try:
            payload = json.loads(self.rfile.read(length))
        except (json.JSONDecodeError, UnicodeDecodeError):
            return self.send_json(400, {"error": "to nie jest JSON"})

        task = payload.get("task")
        if not isinstance(task, str) or not task.strip():
            return self.send_json(400, {"error": "pole task musi być niepuste"})
        resume = bool(payload.get("resume"))
        qs = parse_qs(url.query)
        if qs.get("resume", ["0"])[0] in ("1", "true"):
            resume = True

        with self.run.lock:
            if self.run.busy():
                return self.send_json(409, {**self.run.state(), "error": "zadanie już trwa"})
            if shutil.which(self.binary) is None and not Path(self.binary).is_file():
                return self.send_json(500, {"error": f"nie ma binarki {self.binary}"})
            cmd = [self.binary, "run", "-w", str(self.workspace), "--provider", payload.get("provider", "mock")]
            if resume:
                cmd.append("--resume")
            cmd.append(task)
            marker = self.workspace / ".agent" / "resume"
            before = marker.stat().st_mtime_ns if marker.exists() else 0
            try:
                self.run.proc = subprocess.Popen(
                    cmd,
                    cwd=str(self.workspace),
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
            except OSError as exc:
                self.run.error = str(exc)
                return self.send_json(500, {"error": f"nie mogę uruchomić: {exc}"})
            self.run.started = time.time()
            self.run.code = None
            self.run.error = None
            self.run.session = None
            # Sesja jest tworzona chwilę po starcie: czekamy na zmianę .agent/resume.
            for _ in range(60):
                if marker.exists() and marker.stat().st_mtime_ns != before:
                    self.run.session = marker.read_text().strip() or None
                    break
                time.sleep(0.05)
            return self.send_json(200, self.run.state())


def main() -> int:
    ap = argparse.ArgumentParser(description="Podgląd i sterowanie ArndtAi z przeglądarki")
    ap.add_argument("--workspace", default=".", help="katalog roboczy agenta")
    ap.add_argument("--port", type=int, default=8766)
    ap.add_argument("--bin", default=os.environ.get("ARNDTAI_BIN", "ardntai"))
    opts = ap.parse_args()

    workspace = Path(opts.workspace).resolve()
    if not workspace.is_dir():
        print(f"nie ma katalogu {workspace}", file=sys.stderr)
        return 2

    Handler.workspace = workspace
    Handler.binary = opts.bin
    server = ThreadingHTTPServer(("127.0.0.1", opts.port), Handler)
    print(f"ArndtAi UI: http://127.0.0.1:{opts.port}/web/  (workspace {workspace})", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        with Handler.run.lock:
            if Handler.run.proc is not None and Handler.run.proc.poll() is None:
                Handler.run.proc.terminate()
        print("\nzatrzymane", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
