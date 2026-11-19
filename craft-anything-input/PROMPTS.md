# Kolejka promptów — jeden prompt = jedna rzecz

> **Gdzie pracujesz:** kod jest w `app/` (układ CA „start-from"). Ścieżki `src/...` są relatywne do `app/`; komendy `cargo ...` uruchamiaj w `app/`. W korzeniu leżą tylko materiały (`craft-anything-input/`, `ArndtAi.zip`, `audyty/`) i `craft.json`.

Stan zmierzony 2026-11-13 (dowody, nie pamięć):

| Co | Dowód |
|---|---|
| build OK, 3 warningi | `cargo build` → `Finished`, m.in. `dead_code` dla `proposed/fetch_done/complete/complete_with_outbox/outbox_for_session` i `resolve_open` |
| testy 7/7 | `cargo test` → `test result: ok. 7 passed` |
| demo 39/39 | `cargo run -- demo` → `39/39 sprawdzen przeszlo`, `DEMO OK` |

**Uwaga:** `/home/karndt/projects/ArndtAi` **nie jest repozytorium git** (`git rev-parse` → `fatal: not a git repository`). Nie ma commitów, więc każdy prompt musi kończyć się buildem + testami, a nie `git commit`. Snapshot to `ArndtAi.zip`.

## Kolejność

### 1. Recovery end-to-end — `craft-anything-input/TASK.md`
Dlaczego pierwszy: bez stabilnego `session_id` cała tabela `steps` jest martwym kodem, a to warunek wstępny Fazy B pkt 2–4.
Akceptacja: `cargo build` bez `dead_code`, `cargo test` zielone, `demo` 39/39.

Prompt (wklej):
```
Zrób dokładnie to, co jest w craft-anything-input/TASK.md. Nie czytaj innych plików z tego katalogu.
Skończ, gdy cargo build, cargo test i cargo run -- demo przechodzą. Pokaż wyniki.
```

### 2. Outbox: jeden właściciel
Dziś dwie ścieżki: `notify.send` pisze plik do `.agent/outbox` (`src/tools.rs:446`), a `Store::complete_with_outbox` pisze do tabeli `outbox` i **nikt go nie woła**. Wybierz tabelę (single-writer), usuń plikową ścieżkę albo zostaw jako eksport.
Akceptacja: test, że `notify.send` + `complete_with_outbox` dają jeden rekord, a po restarcie `outbox_for_session` go widzi.

### 3. SQLite: WAL + busy_timeout
`grep WAL src/` → pusto. Wymóg `09-INSTRUKCJA` Fazy B pkt 1.
Akceptacja: `journal_mode` zwraca `wal`, test równoległego odczytu w trakcie zapisu.

### 4. Schemat eventów zamrożony
`craft-anything-input/docs/contracts/` ma tylko `mcp.md`. Frontend nie ma z czego czytać.
Akceptacja: `docs/contracts/events.md` z polami `type, step, ts, payload` + test, że każdy `session.event(...)` w kodzie używa typów z tego pliku.

### 5. `resolve_open` podłączony
`src/tools.rs:232` — druga weryfikacja po `canonicalize` gotowa, ale nieużywana (Faza B pkt 6).
Akceptacja: `fs.read`/`fs.write` otwierają przez `resolve_open`, testy symlink/traversal nadal zielone.

### 6. Frontend (dopiero po 4)
Jeden prompt = jeden ekran. Najpierw lista sesji + ledger kroków z `.agent/sessions/*.jsonl`, potem inspector, potem SSE.

## Czego nie robić

- Nie dokładać faz A+B+C naraz — to był problem poprzedniego podejścia.
- Nie dodawać crate'ów bez zgody (axum dopiero w prompcie 6).
- Nie „przy okazji" refaktoryzować `agent.rs` (330 linii, działa).
