# M1 — domknięcie po gicie: lokalne CI i odporność na kill -9

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Każdy commit niesie świeży dowód (verify.py), a utrata procesu w środku
narzędzia ma test end-to-end zamiast samego wpisu w `GATES.md`.

**Architecture:** Dowód liczy `scripts/verify.py` (już istnieje) i zapisuje go do
`checks/evidence.json` + `craft.json`. Nowy `Makefile` daje jedno wejście
(`make verify`), a `scripts/hooks/pre-commit` uruchamia je przed commitem
i dokłada odświeżone dowody do commita. Test `kill -9` idzie przez prawdziwą
binarkę i `SIGKILL` na procesie zablokowanym w `notify.send` — to jedyne
miejsce, gdzie efekt uboczny jest już w locie, gdy proces ginie.

**Tech Stack:** Rust (rusqlite, ratatui), Python 3 (`scripts/verify.py`),
bash (hook), git.

**Spec:** `docs/STATUS.md` (M1 poz. 2), `docs/handoffs/2026-11-19-grok-odpowiedz.md`,
`docs/contracts/events.md` (dla Fazy C).

## Global Constraints

- Bez nowych zależności (Rust i Python) — decyzja z AGENTS.md.
- Sekret nigdy w `argv`; test `kill -9` używa lokalnego listenera TCP, nie sieci.
- `cargo test` musi kończyć się zerem błędów i bez ostrzeżeń.
- Commit tylko na `main` w `/home/karndt/projects/ArndtAi`; push na `origin`.
- Po każdej zmianie w `app/src/` dowody trzeba odświeżyć (`make verify`).

## Review Focus

- **Hook blokuje commit, gdy test padnie** — użytkownik musi zobaczyć powód,
  nie „exit 1" bez kontekstu.
- **Hook nie wciąga do commita cudzych zmian** — staguje wyłącznie `checks/`
  i `craft.json`.
- **`SIGKILL` trafia w proces zablokowany na `notify.send`**, nie w model —
  inaczej test mierzy inną fazę i przechodzi zawsze.
- **Po `--resume` efekt nie jest powtórzony** — listener widzi dokładnie jedno
  żądanie, także gdy krok był `proposed`.
- **Test nie zostawia procesu-sieroty** — dziecko zabite, listener zamknięty
  w `Drop`/na końcu, inaczej `cargo test` wisi.

---

### Task 1: `make verify` + hook pre-commit

**Files:**
- Create: `Makefile`
- Create: `scripts/hooks/pre-commit`
- Modify: `AGENTS.md` (sekcja „Komendy")

**Interfaces:**
- Produces: `make verify` — exit 0 tylko gdy `verify.py` i `--assert` przeszły;
  `scripts/hooks/pre-commit` — instalowany przez `git config core.hooksPath scripts/hooks`.

- [ ] **Step 1: Napisz `Makefile`**

Cele: `verify` (run + assert), `test` (`cargo test`), `demo`. `verify` wypisuje
linię `VERIFY OK` po sukcesie.

- [ ] **Step 2: Sprawdź, że `make verify` przechodzi na czystym drzewie**

Run: `make verify`
Expected: `CHECKS OK`, `VERIFY OK`, exit 0.

- [ ] **Step 3: Napisz `scripts/hooks/pre-commit` (bash, `set -euo pipefail`)**

Kolejność: `make verify`, potem `git add checks craft.json`, potem `exit 0`.
Przy błędzie `make` — komunikat na stderr z komendą do ręcznego uruchomienia
i `exit 1`.

- [ ] **Step 4: Zainstaluj hook i sprawdź, że blokuje**

Run: `git config core.hooksPath scripts/hooks`, potem celowo zepsuj jeden test
(zmiana w `app/src/store.rs`, potem `git checkout app/src/store.rs`),
`git commit --allow-empty -m probe`.
Expected: commit odrzucony, komunikat z `make verify`.

- [ ] **Step 5: Sprawdź, że przechodzi i dokłada dowody**

Run: `git commit --allow-empty -m "chore: hook pre-commit z verify.py"`
Expected: commit przechodzi, `git show --stat` zawiera `checks/evidence.json`.

- [ ] **Step 6: Dopisz do `AGENTS.md` sekcję o `make verify` i hooku; commit + push**

### Task 2: test `kill -9` na `notify.send`

**Files:**
- Modify: `app/tests/cli.rs` (nowy test + helper listenera)
- Modify: `GATES.md` (G15 z dowodem)

**Interfaces:**
- Consumes: `fake_model` i `run_env` z `tests/cli.rs`; `notify.send` przyjmuje
  `{"url": "...", "message": "..."}` (patrz `docs/contracts/mcp.md`).
- Produces: test `killing_the_process_inside_notify_send_does_not_resend_on_resume`.

- [ ] **Step 1: Napisz listener TCP, który przyjmuje żądanie i nigdy nie odpowiada**

`TcpListener::bind("127.0.0.1:0")`, wątek akceptuje połączenie, czyta nagłówki,
wysyła `mpsc` sygnał „dotarło", potem śpi do końca testu.

- [ ] **Step 2: Napisz czerwony test**

Scenariusz: fake model zwraca krok `notify.send` na listener; uruchom binarkę
jako dziecko; czekaj na sygnał z listenera; `Child::kill()` (SIGKILL);
drugi bieg z `--resume` z modelem zwracającym `done`; asercje: listener dostał
dokładnie 1 żądanie, drugi bieg exit 0, krok 1 w `state.db` nie jest `proposed`.

- [ ] **Step 3: Uruchom test — musi być czerwony z właściwego powodu**

Run: `cargo test --test cli killing_the_process`
Expected: FAIL (dziś brak takiego pokrycia; jeśli przejdzie od razu, sprawdź,
czy SIGKILL naprawdę trafia w zablokowany `notify.send`).

- [ ] **Step 4: Doprowadź do zielonego bez zmiany produkcyjnej, jeśli się da**

Jeśli test ujawni dziurę (np. powtórzony efekt), napraw `agent.rs` minimalnie —
tylko to, co pokazuje test.

- [ ] **Step 5: `make verify`, potem `GATES.md` G15 z hashem i commit + push**

### Task 3: spec Fazy C i brief dla Craft Anything

**Files:**
- Create: `docs/specs/faza-c-web-ui.md`
- Create: `docs/superpowers/plans/2026-11-19-faza-c-web-ui.md` (po akceptacji spec)

**Interfaces:**
- Consumes: `docs/contracts/events.md` (format zdarzeń), `craft.json` (projekt CA).
- Produces: brief gotowy do `ca-delegate` (folder, backend, rundy, komenda rebuild).

- [ ] **Step 1: Spisz spec: jeden ekran (rozmowa + kroki), źródło danych JSONL,
  brak nowych zależności po stronie rdzenia; kryteria PASS.
- [ ] **Step 2: Sprawdź spec z Grokiem (Orca browser) i zapisz jego uwagi.
- [ ] **Step 3: Zleć CA przez `ca-delegate` jako osobny bieg; odbiór przez `ca-accept`.
