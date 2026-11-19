<!-- open-steps:begin -->
_Zmierzone 2026-11-19. Etapy datowane tam, gdzie stoją; wszystko bez daty w tym pliku nie jest zmierzone._

## Czym to jest

ArndtAi to samodzielny agent wykonawczy: dostaje zadanie po polsku, sam planuje
kroki, woła narzędzia (czytanie, pisanie i dopisywanie plików, powiadomienia,
ładowanie skilli) i zapisuje każdy krok w bazie SQLite. Ma bramki zgód, więc
nie każdy zapis przechodzi bez pytania. Obsługuje go się z terminala (TUI)
albo z linii poleceń. Napisany w Rust, buduje się i testuje lokalnie na Linuksie.

## Co w nim jest

| Funkcja | Co robi | Etap | Ostatnia praca | Sygnał |
|---|---|---|---|---|
| pętla decyzyjna z bramkami | planuje krok, sprawdza zakres, wykonuje narzędzie | live (14 lis) | nie sprawdzono | nie sprawdzono |
| trwały stan i wznowienie | SQLite WAL, transkrypty JSONL, `--resume` liczy kroki dalej | live (14 lis) | nie sprawdzono | nie sprawdzono |
| perimetr narzędzi MCP | `fs.read`, `fs.write`, `fs.append`, `notify.send`, `skill.load` | live (14 lis) | nie sprawdzono | nie sprawdzono |
| TUI (ratatui) | powitanie, rozmowa, sesje, `/model`, `/settings`, `/skills`, `/copy`, `/export`, markdown | live (14 lis) | nie sprawdzono | nie sprawdzono |
| skille | `SKILL.md` z trzech katalogów, lista w prompcie, treść przez `skill.load` | live (14 lis) | nie sprawdzono | nie sprawdzono |
| pamięć i wyszukiwanie | FTS5 po transkryptach, poziomy `lite`/`pro`, backfill starej bazy przy migracji (test 19 lis) | live (19 lis) | nie sprawdzono | nie sprawdzono |
| testy na żywo | `scripts/live_test.py`, 8 scenariuszy, prawdziwy model, pty + pyte | live (14 lis) | nie sprawdzono | nie sprawdzono |

Kolumny „Ostatnia praca" i „Sygnał" są **nie sprawdzone**: skrypt pomiarowy
(`census.sh`) odmówił — katalog nie jest repozytorium git. Zmierzone ręcznie
daty zapisu plików: `app/src/` 14 lis 03:07, `docs/STATUS.md` 14 lis 03:09.

## Warte wycofania

Nie znaleziono — brak pomiaru (kolumna „Sygnał" nie istnieje bez git).

## Co dalej

- Faza C, web UI — źródło: `docs/STATUS.md` (M1, poz. 1).
- Perimetr MCP po `SIGKILL` wykonuje krok `done` drugi raz (znalezione testami 19 lis; rdzeń `--no-mcp` poprawny) — źródło: `GATES.md` G15, nota.
- Python core (ADR-004, status Proposed) — źródło: `docs/STATUS.md` (M1, poz. 3).
- Repozytorium git — źródło: `docs/STATUS.md` (M1, poz. 4): `app/` ma 984 MB, brak historii zmian.
- Render markdown w `/export` i `/copy` — źródło: raport builda runda 18 (`build-activity.log`, 14 lis 03:09), „następny krok wart zlecenia".

_Starzenie i okablowanie: nie zmierzone (brak git). Etap pochodzi z `docs/STATUS.md`
z 14 listopada i jest tylko tak świeży, jak data obok niego._
<!-- open-steps:end -->
