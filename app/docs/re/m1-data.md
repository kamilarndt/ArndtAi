# Dane M1: `tasks` / `transcripts` + FTS pamięci (runda 11)

Data: 2026-11-14. Zakres: `app/src/store.rs`, `app/src/context.rs`,
`app/src/agent.rs` (+ testy w tych plikach). Kontrakty (`docs/`), wygląd TUI,
bramki zakresu, detektor dryfu, allowlista i outbox — bez zmian.

## 1. Co doszło do schematu

`Store::init()` tworzy dwie nowe tabele i jedną wirtualną. Wszystko przez
`CREATE ... IF NOT EXISTS`, więc każde otwarcie bazy (także sprzed tej rundy)
jest bezpieczne i idempotentne.

```
tasks(id TEXT PRIMARY KEY, session_id TEXT, text TEXT, status TEXT, created_at INTEGER)
  + indeks tasks_session(session_id)

transcripts(id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT, step INTEGER,
            kind TEXT, payload TEXT, at INTEGER)
  + indeks transcripts_session_step(session_id, step)

transcripts_fts  — FTS5 po `payload` (tabela zewnętrznej treści)
```

- `tasks`: jedna sesja = jedno zadanie, więc `id` = `session_id`. Wiersz
  powstaje w `agent::run` (`start_task`, status `running`), a `finish`
  ustawia status końcowy: `DONE` → `done`, każdy `FAILED_*` → `failed`
  (`task_status`). `--resume` trafia w ten sam `id`: status wraca na
  `running`, a `INSERT OR IGNORE` nie podmienia pierwotnego tekstu zadania.
- `transcripts`: `payload` to **cała linia eventu** — dokładnie ta sama, która
  leci do JSONL (`{"at":…,"kind":…,"data":{…}}`), więc zapis jest bezstratny,
  a `kind` się zgadza. `step` jest wynoszony z `data.step` (dla `task`,
  `summary`, `store` itp. zostaje `NULL`).
- `at` to czas zapisu wiersza (`now_ts`), nie czas z linii eventu; czas eventu
  jest w `payload`.

## 2. Jak wygląda synchronizacja

Zapis: `Session::event` (jedyne miejsce, w którym powstaje linia JSONL)
po zapisaniu pliku woła `Store::append_transcript`. JSONL jest zapisem
pierwszym i nadal eksportem/kontraktem; baza jest indeksem. Błąd zapisu do
bazy nie zatrzymuje pętli (ignorowany), a `append_transcript` sam doraobi
schemat (`init`) gdyby sesja pisała przed `Store::init` — dlatego sesja
utworzona w dowolnym momencie ma wiersz na każdą linię JSONL.

FTS: trzy triggery na `transcripts` — `transcripts_ai` (insert),
`transcripts_au` (update: `delete` + `insert`), `transcripts_ad` (delete) —
trzymają `transcripts_fts` zgodnie z tabelą treści. Nie ma drugiej kopii
tekstu: FTS5 z `content='transcripts'` czyta `payload` z tabeli głównej.

Migracja istniejącej bazy: `PRAGMA user_version` = 0 → jedno
`INSERT INTO transcripts_fts(transcripts_fts) VALUES('rebuild')` i
`user_version = 1`. Na starej bazie `transcripts` jest pusta, więc przebudowa
jest darmowa; na nowej bazie ten krok nie wykonuje się nigdy więcej.

Wyszukiwanie: `Store::search_transcripts(query)` →

```
- puste zapytanie (albo samo `::`, `;`, wielokropek) → pusta lista, bez błędu
- inaczej: każdy token alfanumeryczny w cudzysłowie, połączone `OR`,
  ORDER BY bm25, LIMIT 8, snippet z nawiasami [ ] wokół trafienia
```

Cudzysłowy i `OR` są po to, żeby znaki specjalne FTS (`"`, `*`, `:`, `-`)
z tekstu zadania nie wysadzały `MATCH`.

Poziom `pro` (i `ultra`) dokłada do kontekstu sekcję
`=== PAMIEC: TRAFIENIA Z HISTORII (FTS) ===` z maksymalnie 8 trafieniami
(`sesja <id> krok <n>: <snippet>`), dopisywaną po `Prefix::render()`. Brak
trafień = brak sekcji. Poziom `lite` nic nie czyta z historii.

## 3. Koszty (zmierzone)

- Sesja `write a.txt :: alfa; read a.txt` (10 eventów): JSONL 1 113 B, baza
  80 KB — dolny rozmiar strony SQLite (4 KB × strony + FTS + WAL), więc dla
  małych sesji o rozmiarze decyduje narzut stały, nie treść.
- 500 wierszy `transcripts` po ~496 B payloadu (tekst syntetyczny, 60 słów):
  tabela bez FTS 319 488 B, z FTS 417 792 B → **FTS to +31 % tabeli**, cała
  baza ≈ 1,68 × bajtów JSONL. Dla tekstu naturalnego narzut jest podobny
  (indeks rośnie z liczbą unikalnych tokenów).
- Do tego dochodzi `-wal`/`-shm` obok `state.db` (WAL już był) i wpis na
  każdy event — przy 1 000 eventów rzędu kilkuset KB.
- Koszt zapisu: jedno `INSERT` na event (osobne połączenie, WAL) — bez
  transakcji zbiorczej; przy tej skali (dziesiątki eventów na przebieg) to
  niezauważalne.

## 4. Dowody

- `cargo build` — exit 0, bez ostrzeżeń.
- `cargo test` — 110 testów jednostkowych + 4 integracyjne, wszystkie zielone.
  Nowe: `old_database_opens_and_gains_tasks_and_transcripts`,
  `task_row_keeps_text_and_final_status`,
  `transcripts_mirror_every_jsonl_event`,
  `fts_follows_insert_update_and_delete`, `empty_search_is_not_an_error`,
  `pro_level_appends_history_hits_to_context`.
- `cargo run -- demo` — 39/39, `DEMO OK`.
- Przebieg na realnym workspace (`--memory pro`): `tasks` = 1 wiersz
  (`status='done'`, tekst zadania), `transcripts` 10 = 10 linii JSONL,
  `MATCH '"alfa"'` zwraca trafienia z krokami 1 i 2. Po `--resume`:
  16 = 16 linii, tekst zadania bez zmian, `a.txt` = `alfabeta`.
- `cargo run -- --screenshot app/shots/now.png` — PNG bez zmian w układzie.

## 5. Czego świadomie nie zrobiono

- **Brak backfillu historii**: stare sesje mają JSONL, ale nie mają wierszy
  w `transcripts` (tylko eventy zapisane po tej rundzie). Backfill z plików
  to osobny krok — tutaj migracja dotyczy schematu, nie treści.
- `sessions.task` zostaje (nie usuwamy kolumny): `tasks` jest nowym źródłem
  prawdy o zadaniu, ale stara kolumna nie przeszkadza i nie łamie czytających.
- Brak dwukierunkowej synchronizacji: ręczna zmiana JSONL nie zmienia bazy
  i odwrotnie. JSONL jest eksportem, baza indeksem — zgodnie z AGENTS.md.
- Brak wyszukiwania w TUI i w `status` (wskaźnik tury i UI to kosmetyka,
  poza zakresem tej rundy); `search_transcripts` jest gotowe dla Fazy C
  i poziomu `pro`.
- Bez własnego tokenizera FTS (domyślny `unicode61`), bez `porter` — polskie
  końcówki nie są sprowadzane do rdzenia; szukanie działa po tokenach.
- Kontrakty (`docs/contracts/*`) i sekcja „Odstępstwa" w `events.md` —
  nietknięte, zgodnie z poleceniem.
