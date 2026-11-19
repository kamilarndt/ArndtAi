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
    "gate",
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
        ev(10, "gate", {"stage": "POLICY_CHECK", "detail": "notify.send"}),
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


MD_SAMPLE = """# Nagłówek poziomu 1
## Nagłówek poziomu 2
### Nagłówek poziomu 3

Akapit z **pogrubieniem**, *kursywą* i `kodem inline` — atrybuty, nie znaki.
Druga linia akapitu: twarde złamanie, tak jak w TUI.

- punkt pierwszy
- punkt drugi z `kodem`
  - punkt zagnieżdżony
1. krok numerowany
2. drugi krok numerowany
3. trzeci krok

> Cytat: bramka odrzuciła zapis.
>> Głębiej: powód jest w evencie `denied`.

| narzędzie | rola | stan |
|---|---|---|
| fs.write | zapis pliku | ok |
| fs.read | odczyt pliku | **ok** |
| notify.send | powiadomienie | ok |

Obraz ![schemat](shot.png) i link [zasady](docs/AGENTS.md) zostają tekstem.
HTML <b>bold</b>, LaTeX $x^2$ i mermaid graph TD też. Emoji 🚀 bez zmian."""

MD_CODE = """Blok kodu z podświetlaniem — ramka i etykieta języka:

```rust
fn main() {
    let krok: u32 = 12;
    // komentarz: numer kroku w evencie
    println!("krok {}", krok);
}
```

```json
{"kind": "tool_call", "step": 1, "ok": true, "bytes": 40}
```

```bash
cargo build --release && cargo test --quiet   # 232 testy w rdzeniu
```

```toml
[store]
path = ".agent/state.db"
busy_timeout = 5000  # ms
```

```bash
cargo run --quiet -- -w /tmp/ws "write a.txt :: tresc; read a.txt" --steps 4 --memory lite --provider mock  # linia dluzsza niz 120 znakow, zeby pokazac obciecie
```"""

MD_BROKEN = """Niepełny markdown nie wywraca widoku: **pogrubienie bez końca, *kursywa bez końca i `kod bez końca.

```rust
// blok bez zamknięcia — reszta zostaje w ramce jako tekst
let x = 1;"""


def long_block() -> str:
    """Blok kodu dłuższy niż 200 linii — dowód obcięcia z `… (N więcej)`."""
    lines = ["Blok dłuższy niż 200 linii:", "", "```bash"]
    for i in range(1, 206):
        lines.append(f"echo 'linia {i:03d}'  # krok {i} przebiegu")
    lines.append("```")
    return "\n".join(lines)


def markdown_events(base: int) -> list[dict[str, Any]]:
    """Sesja z próbką każdego elementu markdownu (runda 20).

    Markdown jest w `thought` i `summary.reason`; `task` i `tool_result.output`
    celowo zawierają `**` i `#`, żeby było widać, że zostają dosłowne.
    """

    def ev(offset: int, kind: str, data: dict[str, Any]) -> dict[str, Any]:
        return {"at": base + offset, "kind": kind, "data": data}

    return [
        ev(0, "task", {"text": "write notatka.txt :: alfa; read notatka.txt  (**to nie markdown**)"}),
        ev(1, "store", {"path": ".agent/state.db", "journal_mode": "wal", "busy_timeout": 5000}),
        ev(2, "thought", {"step": 1, "thought": MD_SAMPLE}),
        ev(3, "tool_call", {"step": 1, "tool": "fs.write", "args": {
            "path": "notatka.txt", "content": "alfa **bez renderu**",
        }}),
        ev(4, "tool_result", {"step": 1, "tool": "fs.write", "ok": True, "code": None, "bytes": 4,
                              "output": {"content": "**wynik narzedzia** zostaje doslowny: # nie naglowek"}}),
        ev(5, "thought", {"step": 2, "thought": MD_CODE}),
        ev(6, "thought", {"step": 3, "thought": MD_BROKEN}),
        ev(7, "thought", {"step": 4, "thought": long_block()}),
        ev(8, "summary", {
            "status": "DONE", "steps": 4, "tool_calls": 1, "denied": 0, "drift": 0,
            "reason": "**cztery** kroki, zero odmów — `status` bez zmian.",
        }),
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description="Sesja JSONL z 12 typami eventów (kontrakt v1).")
    parser.add_argument("--out", required=True, help="katalog docelowy (powstanie, jeśli go nie ma)")
    parser.add_argument("--pad", type=int, default=0, help="ile dodatkowych eventów dołożyć (domyślnie 0)")
    parser.add_argument("--name", default=None, help="nazwa pliku bez .jsonl (domyślnie fixture-12 / fixture-md)")
    parser.add_argument(
        "--markdown",
        action="store_true",
        help="sesja z próbką markdownu (nagłówek, pogrubienie, kursywa, kod, listy, cytat, tabela, niepełny)",
    )
    args = parser.parse_args()

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    name = args.name or ("fixture-md" if args.markdown else "fixture-12")
    path = out_dir / f"{name}.jsonl"

    base = int(time.time()) - 120
    if args.markdown:
        events = markdown_events(base)
    else:
        events = build(base, max(0, args.pad))
    with path.open("w", encoding="utf-8") as handle:
        for event in events:
            handle.write(json.dumps(event, ensure_ascii=False, sort_keys=True) + "\n")

    if args.markdown:
        print(f"{path}")
        print(f"{len(events)} linii, probka markdownu w thought + summary.reason")
        print("elementy: naglowki 1-3, pogrubienie, kursywa, kod inline, listy (- i 1.),")
        print("          bloki rust/json/bash/toml, dluga linia, cytat, tabela, niepelny markdown,")
        print("          blok 205 linii (obciecie), doslowny task i tool_result")
        return 0

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
