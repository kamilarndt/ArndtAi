# PLAN-DOKONCZENIA — ArndtAi

**Data:** 2026-11-13  
**Status:** M0 + Faza B zamkniete. TUI w kodzie, bez bramki.

## 1. Co znaczy "koniec" tego etapu

**Koniec = M0 + Faza B + TUI z bramka.**

- Petla, bramki, MCP, SQLite recovery, sandbox — **gotowe** (G1-G3).
- TUI (ratatui) — kod jest, brak dowodu w GATES.
- Web UI (Faza C), FTS, tabele tasks/transcripts, testy kill -9 — **nie wchodza** do tego etapu (M1).

Po G4 (TUI) etap jest zamkniety. Reszta to M1.

## 2. Kolejnosc

| # | Pozycja | Etap | Uwagi |
|---|---------|------|-------|
| 1 | Bramka TUI w GATES.md | **teraz** | jedyna pozycja do tego etapu |
| 2 | Tabele tasks/transcripts | M1 | odstępstwo w events.md |
| 3 | FTS pamieci Pro | M1 | |
| 4 | Testy kill -9 | M1 | opcjonalne |
| 5 | Faza C web UI | M1 | |
| 6 | Python core | M1 | ADR-004 Proposed |

## 3. Kryteria akceptacji (sprawdzalne bez Ciebie)

### G4 — TUI dziala (teraz)

- **CHECK:** `cd app && cargo build --quiet && cargo run --quiet -- tui --provider mock --steps 2 "write a.txt :: x" 2>&1 | head -5`
- **EXPECT:** exit code 0, brak panic, w stdout/stderr brak `error[`
- **DOWOD:** screenshot TUI (960x608 lub wiekszy) zapisany jako `checks/tui-screenshot.png` + hash w GATES.md
- **CWD:** `app`
- **Dodatkowo:** `cargo test --quiet` nadal exit 0

### M1 — nie teraz (kryteria na pozniej)

- **tasks/transcripts:** migracja SQLite + wiersz w `events.md` „Odstępstwa” usuniety; `cargo test` ok.
- **FTS:** `SELECT` po tresci kroku zwraca wynik; test jednostkowy.
- **kill -9:** test uruchamia proces, SIGKILL w srodku toola, resume nie powtarza efektu; exit 0.
- **Web UI:** serwuje eventy z `.agent/sessions/*.jsonl`, trajektoria widoczna.
- **Python core:** 5 golden scenarios + odpowiednik demo przechodza na nowym rdzeniu; kontrakty bez zmian.

## 4. Co zrobic teraz (dla Craft Anything)

1. Uruchom CHECK z G4.
2. Zrob screenshot TUI.
3. Dopisz do `GATES.md`:

```
- [x] G4: TUI otwiera sie i wykonuje zadanie (mock)
  CHECK: cd app && cargo build --quiet && cargo run --quiet -- tui --provider mock --steps 2 "write a.txt :: x"
  EXPECT: exit 0
  CWD: app
  EVIDENCE: (wklej hash screenshotu i exit code)
```

4. Nie ruszaj web UI, FTS, tasks, Python core.

Po G4 — etap zamkniety.
