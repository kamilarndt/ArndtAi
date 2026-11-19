#!/usr/bin/env python3
"""Generator sesji testowej dla web UI (Faza C).

Tworzy plik JSONL zgodny z kontraktem `docs/contracts/events.md` (v1) z
dokładnie jednym eventem każdego z 12 typów:

    python3 app/web/fixture_events.py --out /tmp/fx

daje `/tmp/fx/fixture-12.jsonl` (12 linii). `--pad N` dokłada N dodatkowych
eventów (thought / tool_call / tool_result), żeby sprawdzić filtr i szukanie
na sesji z 100+ wpisami; domyślnie plik ma dokładnie 12 linii.

Skrypt tylko pisze plik wzorcowy — nie dotyka `.agent/` ani niczego w repo.
"""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path
from typing import Any


KINDS = [
    "task",
    "thought",
    "tool_call",
    "tool_result",
    "tool_result_cached",
    "tool_result_skipped",
    "denied",
    "store",
    "store_error",
    "error",
    "summary",
    "repair",
]


def core_events(base: int) -> list[dict[str, Any]]:
    """Po jednym evencie każdego typu, w kolejności prawdziwego przebiegu."""
    def ev(offset: int, kind: str, data: dict[str, Any]) -> dict[str, Any]:
        return {"at": base + offset, "kind": kind, "data": data}

    return [
        ev(0, "task", {"text": "write notatka.txt :: alfa; read notatka.txt"}),
        ev(1, "store", {"path": ".agent/state.db", "journal_mode": "wal", "busy_timeout": 5000}),
        ev(2, "thought", {"step": 1, "thought": "krok 1 z planu: fs.write"}),
        ev(3, "tool_call", {"step": 1, "tool": "fs.write", "args": {"path": "notatka.txt", "content": "alfa"}}),
        ev(4, "tool_result", {
            "step": 1, "tool": "fs.write", "ok": True, "code": None, "bytes": 4,
            "output": {"bytes_written": 4},
        }),
        ev(5, "tool_result_cached", {"step": 1, "tool": "fs.write", "result": {"bytes_written": 4}}),
        ev(6, "tool_result_skipped", {"step": 2, "tool": "notify.send", "reason": "crash_before_result"}),
        ev(7, "denied", {
            "step": 3, "tool": "fs.write", "drift": True,
            "reason": "fs.write odrzucone przez bramke zakresu: DENIED: sciezka zawiera '..'",
        }),
        ev(8, "store_error", {"stage": "write", "message": "database is locked (busy_timeout 5000 ms)"}),
        ev(9, "repair", {"step": 4, "detail": 'proba 1/2: brak konca JSON-a; ogon: "..."'}),
        ev(10, "error", {"message": "model zwrocil niepoprawny JSON po jednej naprawie"}),
        ev(11, "summary", {
            "status": "FAILED_ERROR", "steps": 4, "tool_calls": 1, "denied": 1,
            "drift": 1, "reason": "dwie nieudane odpowiedzi modelu",
        }),
    ]


def filler_events(base: int, count: int) -> list[dict[str, Any]]:
    """Dodatkowe, poprawne eventy, żeby sesja miała 100+ linii."""
    out: list[dict[str, Any]] = []
    for i in range(count):
        step = 100 + i
        out.append({"at": base + 12 + i, "kind": "thought", "data": {
            "step": step, "thought": f"przygotowanie {i + 1}: sprawdzam plik roboczy",
        }})
        out.append({"at": base + 12 + i, "kind": "tool_call", "data": {
            "step": step, "tool": "fs.read", "args": {"path": f"pliki/roboczy-{i + 1}.txt"},
        }})
        out.append({"at": base + 12 + i, "kind": "tool_result", "data": {
            "step": step, "tool": "fs.read", "ok": i % 7 != 0, "code": None, "bytes": 12,
            "output": {"content": f"linia {i + 1}", "size": 12},
        }})
    return out


def build(base: int, pad: int) -> list[dict[str, Any]]:
    events = core_events(base)
    if pad <= 0:
        return events
    # Fillery wchodzą przed `summary` (kontrakt: podsumowanie jest ostatnie),
    # a znacznik czasu podsumowania przesuwa się za nie, żeby plik był
    # chronologiczny także po `at`.
    summary = dict(events[-1])
    summary["at"] = base + 12 + pad + 60
    return events[:-1] + filler_events(base, pad) + [summary]


def main() -> int:
    parser = argparse.ArgumentParser(description="Sesja JSONL z 12 typami eventów (kontrakt v1).")
    parser.add_argument("--out", required=True, help="katalog docelowy (powstanie, jeśli go nie ma)")
    parser.add_argument("--pad", type=int, default=0, help="ile dodatkowych eventów dołożyć (domyślnie 0)")
    parser.add_argument("--name", default="fixture-12", help="nazwa pliku bez .jsonl (domyślnie fixture-12)")
    args = parser.parse_args()

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / f"{args.name}.jsonl"

    base = int(time.time()) - 120
    events = build(base, max(0, args.pad))
    with path.open("w", encoding="utf-8") as handle:
        for event in events:
            handle.write(json.dumps(event, ensure_ascii=False, sort_keys=True) + "\n")

    kinds = {event["kind"] for event in events}
    print(f"{path}")
    print(f"{len(events)} linii, {len(kinds)} typow eventow")
    missing = [kind for kind in KINDS if kind not in kinds]
    if missing:
        print("brak typow: " + ", ".join(missing))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
