# Kontrakt eventów — v2 (2026-11-19)

Zmiana v1 → v2: dodany typ `gate` (przejścia bramek w transkrypcie, wymóg
`05-LOOP-AND-GATES.md`). Reszta bez zmian; konsument v1 czyta v2 bez błędu
(nieznany `kind` pomija), ale traci telemetrię bramek.

Źródło prawdy: `app/src/context.rs` (`Session::event`) i `app/src/agent.rs`.
Każdy event to jedna linia JSONL w `.agent/sessions/<session_id>.jsonl`:

```json
{"at": 1794534331, "kind": "tool_call", "data": {"step": 1, "tool": "fs.write", "args": {"path": "a.txt", "content": "x"}}}
```

- `at` — uniksowy czas w sekundach (u64),
- `kind` — jeden z typów poniżej,
- `data` — obiekt; pola wymagane są wyszczególnione.

Konsument (frontend Fazy C) czyta wyłącznie ten format. Dodanie typu = nowa
wersja tego pliku, nie cicha zmiana.

## Typy eventów

| `kind` | Kiedy | `data` |
|---|---|---|
| `task` | start sesji | `text` — zadanie |
| `thought` | model zwrócił decyzję | `step`, `thought` |
| `tool_call` | przed wykonaniem narzędzia | `step`, `tool`, `args` |
| `tool_result` | po wykonaniu | `step`, `tool`, `ok`, `code`, `bytes`, `output` |
| `tool_result_cached` | krok `done` odczytany z SQLite (wznowienie) | `step`, `tool`, `result` |
| `tool_result_skipped` | krok `proposed` po crashu — at-most-once | `step`, `tool`, `reason` |
| `denied` | bramka allowlisty/budżetu odrzuciła akcję | `step`, `tool`, `reason`, `drift` |
| `store` | stan magazynu na starcie | `path`, `journal_mode`, `busy_timeout` |
| `store_error` | błąd SQLite (nie zabija pętli) | `stage`, `message` |
| `error` | zakończenie z błędem | `message` |
| `repair` | jedna próba naprawy odpowiedzi modelu | `step`, `detail` |
| `gate` | przejście etapu pętli (v2) | `stage`, `detail` |
| `summary` | zawsze na końcu | `status`, `steps`, `tool_calls`, `denied`, `drift`, `reason` |

`stage` w `gate`: `RECEIVED`, `PLANNING`, `STEP_PROPOSED`, `SCOPE_CHECK`,
`POLICY_CHECK`, `EXECUTING`, `OBSERVE`, `STOP_CHECK`. Kolejność w pliku jest
kolejnością wykonania; TUI tych eventów nie rysuje (telemetria), web UI rysuje.

`status` w `summary`: `DONE`, `FAILED_BUDGET`, `FAILED_STALLED`, `FAILED_ERROR`,
`FAILED_STEPS`.

## Stan trwały (SQLite `.agent/state.db`)

Eventy to log; stan odtwarzalny jest w tabelach. Kontrakt między nimi:

| tabela | kolumny | rola |
|---|---|---|
| `sessions` | `id`, `task`, `started_at` | jedna sesja = jeden plik JSONL |
| `steps` | `session_id`, `step_id`, `tool`, `args_json`, `status`, `result_json` | `status` ∈ `proposed`/`done`/`abandoned`; klucz `(session_id, step_id)` |
| `outbox` | `session_id`, `step_id`, `payload`, `created_at` | unikalny per krok (`outbox_step_once`) |

Reguły:

1. `proposed` przed efektem ubocznym, `done` + `result_json` po nim — jedna transakcja z outboxem, gdy narzędziem jest `notify.send`.
2. Wznowienie: `done` → wynik z `result_json`, zero wykonania. `proposed` → `abandoned`, zero wykonania (at-most-once).
3. Jeden proces pisze (`PRAGMA journal_mode=WAL`, `busy_timeout=5000`); serwer MCP czyta i dopisuje własne kroki, ale nigdy nie dotyka cudzych wierszy.

## Zdarzenie `repair` (od 2026-11-14)

Gdy odpowiedź modelu nie jest poprawnym JSON-em, agent wysyła **jedno** dodatkowe
żądanie („zwróć wyłącznie poprawny JSON") i zapisuje to w transkrypcie:

```json
{"at": 1794616000, "kind": "repair", "data": {"step": 3, "detail": "próba 1/2: brak konca JSON-a; ogon: \"...\""}}
```

- `step` — numer kroku, którego dotyczy naprawa,
- `detail` — numer próby i ogon surowej odpowiedzi (diagnostyka, nie treść użytkownika).
- Maksymalnie **jedna** naprawa na krok. Druga nieudana odpowiedź kończy się
  `FAILED_ERROR`, a `detail` zawiera ogon odpowiedzi z drugiej próby.

Konsumenci (web UI, inspektor) mogą ignorować to zdarzenie; nie zmienia ono
kolejności pozostałych eventów.

## Tabele M1 (od 2026-11-14)

Dokument `09-INSTRUKCJA-DOKONCZENIA.md:40-42` wymagał `tasks`, `sessions`,
`transcripts`, `outbox`. Stan po rundzie M1 (2026-11-14):

| tabela | kolumny | rola |
|---|---|---|
| `tasks` | `id`, `session_id`, `text`, `status`, `created_at` | zadanie jako wiersz; `id` = `session_id` tury, która je zaczęła |
| `transcripts` | `id`, `session_id`, `step`, `kind`, `payload`, `at` | przeszukiwalna kopia eventów JSONL (JSONL pozostaje eksportem) |
| `transcripts_fts` | FTS5 po `payload` | indeks wyszukiwania, synchronizowany triggerami |
| `turns` | — | rozróżnienie tura skończona / przerwana (od rundy 10) |

Zasady:

1. **JSONL jest kontraktem, SQLite jest indeksem.** Kolejność zapisu: event do
   JSONL, potem wiersz w `transcripts`; brak wiersza nie unieważnia transkryptu.
2. **FTS obejmuje eventy od wdrożenia M1.** Stara historia nie jest przepisana —
   uzupełnienie (backfill) to osobna decyzja, nie wymóg kontraktu.
3. **Puste zapytanie FTS zwraca pustą listę**, nie błąd (składnia `MATCH ''` jest
   niepoprawna w SQLite, więc funkcja odsiewa je przed wykonaniem).
4. Poziom pamięci `pro` dokłada do kontekstu do 8 trafień z poprzednich sesji.
