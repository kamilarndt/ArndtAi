#!/usr/bin/env python3
"""Dowody do Craft Anything: uruchamia realne komendy i zapisuje ich wynik.

Audyt 2026-11-13 (F12) wykazal, ze dotychczasowe wpisy `checks` byly puste
(`artifacts: []`), czesc miala duplikaty `at_ms`, a checki poprzedzaly kod.

Podzial odpowiedzialnosci (wazne):
- `checks/evidence.json` - pelny dowod: exit_code, hash wyjscia, hash drzewa
  kodu, czas, komenda, artefakt logu. Tego pliku Craft Anything nie dotyka.
- `craft.json` -> `checks` - tylko pola, ktore CA zna i zachowuje przy zapisie
  (`round`, `check`, `passed`, `similarity`, `note`, `artifacts`, `at_ms`).
  CA nadpisuje ten plik po kazdym przebiegu buildera, wiec trzymanie tam
  `exit_code` i hashy bylo z gory skazane na skasowanie.

Uzycie:
    python3 scripts/verify.py            # uruchom i zapisz dowody
    python3 scripts/verify.py --assert   # sprawdz spojnosc dowodow i craft.json
"""
from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "app"
CRAFT = ROOT / "craft.json"
CHECKS_DIR = ROOT / "checks"
EVIDENCE = CHECKS_DIR / "evidence.json"

COMMANDS = [
    ("build", ["cargo", "build", "--quiet"], APP),
    ("own-tests", ["cargo", "test", "--quiet"], APP),
    ("demo", ["cargo", "run", "--quiet", "--", "demo"], APP),
    ("security", ["cargo", "test", "--quiet", "tools::tests"], APP),
    ("durability", ["cargo", "test", "--quiet", "agent::tests"], APP),
]
NAMES = {n for n, _, _ in COMMANDS}


def tree_hash() -> str:
    """Hash drzewa kodu: sciezki i tresc plikow, ktore buduja binarke."""
    h = hashlib.sha256()
    files = sorted(list((APP / "src").rglob("*.rs")) + [APP / "Cargo.toml"])
    for path in files:
        h.update(path.relative_to(APP).as_posix().encode())
        h.update(b"\0")
        h.update(path.read_bytes())
        h.update(b"\0")
    return h.hexdigest()


def run_check(name: str, argv: list[str], cwd: Path, at_ms: int) -> dict:
    started = time.time()
    proc = subprocess.run(argv, cwd=cwd, capture_output=True, text=True)
    duration = round((time.time() - started) * 1000)
    output = (proc.stdout or "") + (proc.stderr or "")
    CHECKS_DIR.mkdir(exist_ok=True)
    log = CHECKS_DIR / f"{at_ms}-{name}.log"
    log.write_text(output)
    return {
        "check": name,
        "passed": proc.returncode == 0,
        "exit_code": proc.returncode,
        "command": " ".join(argv),
        "cwd": str(cwd.relative_to(ROOT)),
        "duration_ms": duration,
        "output_sha256": hashlib.sha256(output.encode()).hexdigest(),
        "output_bytes": len(output.encode()),
        "output_tail": output.strip().splitlines()[-1][:200] if output.strip() else "",
        "tree_sha256": tree_hash(),
        "artifact": str(log.relative_to(ROOT)),
        "at_ms": at_ms,
    }


def write_evidence(evidence: list[dict]) -> None:
    CHECKS_DIR.mkdir(exist_ok=True)
    EVIDENCE.write_text(
        json.dumps(
            {
                "generated_at_ms": int(time.time() * 1000),
                "tree_sha256": tree_hash(),
                "checks": evidence,
            },
            indent=2,
            ensure_ascii=False,
        )
        + "\n"
    )


def write_craft(evidence: list[dict]) -> None:
    data = json.loads(CRAFT.read_text()) if CRAFT.exists() else {}
    # Checki CA, ktorych nie uruchamiamy (np. app-screenshot), zostaja jako zapis.
    foreign = [c for c in (data.get("checks") or []) if c.get("check") not in NAMES]
    ours = [
        {
            "round": 1,
            "check": e["check"],
            "passed": e["passed"],
            "similarity": None,
            "note": f"`{e['command']}` w {e['cwd']}/ -> exit {e['exit_code']}"
            f" ({e['duration_ms']} ms, sha256 {e['output_sha256'][:12]})",
            "artifacts": [e["artifact"]],
            "at_ms": e["at_ms"],
        }
        for e in evidence
    ]
    failed = [e["check"] for e in evidence if not e["passed"]]
    data["checks"] = ours + foreign
    data["status"] = "ready" if not failed else "stopped"
    data["note"] = (
        f"Ready: {len(evidence)} checks pass; evidence in checks/evidence.json"
        if not failed
        else f"Failed: {', '.join(failed)}"
    )
    data["updated_ms"] = int(time.time() * 1000)
    CRAFT.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")


def run() -> int:
    base = int(time.time() * 1000)
    evidence = []
    for i, (name, argv, cwd) in enumerate(COMMANDS):
        item = run_check(name, argv, cwd, base + i)
        print(f"  {'OK' if item['passed'] else 'FAIL':4} {name:12} exit={item['exit_code']} {item['duration_ms']}ms")
        evidence.append(item)
    write_evidence(evidence)
    write_craft(evidence)
    failed = [e["check"] for e in evidence if not e["passed"]]
    print(f"checks: {len(evidence) - len(failed)}/{len(evidence)} passed")
    return 0 if not failed else 1


def verify() -> int:
    problems: list[str] = []
    if not EVIDENCE.exists():
        print("FAIL: brak checks/evidence.json (uruchom scripts/verify.py)")
        return 1
    ev = json.loads(EVIDENCE.read_text())
    items = ev.get("checks") or []
    current = tree_hash()
    if ev.get("tree_sha256") != current:
        problems.append("evidence.json policzony dla innego drzewa kodu")
    names = [i.get("check") for i in items]
    for expected in NAMES:
        if expected not in names:
            problems.append(f"brak dowodu dla checku {expected}")
    stamps = [i.get("at_ms") for i in items]
    if len(set(stamps)) != len(stamps):
        problems.append("duplikaty at_ms w evidence.json")
    for i in items:
        name = i.get("check", "?")
        if not i.get("passed") or i.get("exit_code") != 0:
            problems.append(f"{name}: exit_code != 0")
        if not i.get("output_sha256"):
            problems.append(f"{name}: brak hash wyjscia")
        if i.get("tree_sha256") != current:
            problems.append(f"{name}: dowod dla innego drzewa kodu")
        art = i.get("artifact")
        if not art or not (ROOT / art).is_file():
            problems.append(f"{name}: brak artefaktu {art}")
        elif hashlib.sha256((ROOT / art).read_bytes()).hexdigest() != i.get("output_sha256"):
            problems.append(f"{name}: artefakt {art} nie zgadza sie z hashem wyjscia")

    if not CRAFT.exists():
        problems.append("brak craft.json")
    else:
        craft = json.loads(CRAFT.read_text())
        checks = craft.get("checks") or []
        by_name = {c.get("check"): c for c in checks}
        for i in items:
            name = i.get("check")
            c = by_name.get(name)
            if c is None:
                problems.append(f"craft.json: brak checku {name}")
                continue
            if c.get("passed") != i.get("passed"):
                problems.append(f"craft.json: {name} passed={c.get('passed')} != {i.get('passed')}")
            if i.get("artifact") not in (c.get("artifacts") or []):
                problems.append(f"craft.json: {name} nie wskazuje artefaktu {i.get('artifact')}")
            if not c.get("at_ms"):
                problems.append(f"craft.json: {name} bez at_ms")
        stamps = [c.get("at_ms") for c in checks]
        if len(set(stamps)) != len(stamps):
            problems.append("craft.json: duplikaty at_ms")
        if craft.get("status") != "ready":
            problems.append(f"craft.json: status={craft.get('status')} (oczekiwano ready)")

    if problems:
        for p in problems:
            print(f"  FAIL {p}")
        return 1
    print("CHECKS OK")
    return 0


if __name__ == "__main__":
    if "--assert" in sys.argv[1:]:
        sys.exit(verify())
    sys.exit(run())
