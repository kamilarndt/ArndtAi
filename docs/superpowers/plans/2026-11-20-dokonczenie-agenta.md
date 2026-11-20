# Dokończenie agenta ArndtAi — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Domknąć ArndtAi do stanu, w którym każda pozycja specu 00–09 ma implementację albo świadomą, zapisaną decyzję o wyłączeniu.

**Architecture:** Kod zostaje w Rust (`app/src`), bez nowych zależności. Pamięć: pliki w `.agent-memory/` (`facts.md`, `episodes.md`, `graph.md`, `vault/`). Proaktywność: reguły JSON w `policies/`, wyzwalacze lokalne, sesje robocze przez `worker-` prefix. Self-improvement: propozycje plików, nigdy automatyczne stosowanie.

**Tech Stack:** Rust 2021, `serde_json`, `rusqlite` (bez zmian), Python 3 stdlib dla warstwy UI (`app/web/serve.py`).

**Spec:** `craft-anything-input/00-OVERVIEW.md` … `09-INSTRUKCJA-DOKONCZENIA.md`, audyt `docs/audits/spec-gap-2026-11-19.md`.

## Global Constraints

- Zero nowych zależności w `app/Cargo.toml`.
- Kontrakty `docs/contracts/mcp.md` i `events.md` zmieniane tylko przez podniesienie wersji.
- Każdy task: test najpierw, potem implementacja, na końcu `make verify` i commit.
- Rdzeń nie pisze poza `workspace`; perimetr nie pisze poza tabelę `steps` i `trigger_inbox`.
- Format reguł: JSON (odstępstwo od YAML w spec 06, zapisane w `policies/README.md`).
- Bramka: `make verify`; hooki `pre-commit` i `pre-push`; hosted CI nieaktywne (billing GitHuba).

## Review Focus

1. `memory.ingest` na ścieżce spoza workspace — musi odmówić kodem `DENIED`, nie skopiować pliku.
2. Konsolidacja przy pustych danych — nie może utworzyć `graph.md` z samym nagłówkiem ani wywalić się na braku plików.
3. `tick --run` przy trwającym zadaniu — musi odmówić (lock), nie kolejkować w nieskończoność.
4. `improve` bez transkryptów — propozycja pusta, zero plików zapisanych.
5. Sesje `worker-*` w UI i `status` — muszą być czytane jak zwykłe sesje (parser nie może się wywracać na prefiksie).

---

### Task 1: Konsolidacja Ultra (`ardntai consolidate`)

**Files:**
- Create: `app/src/consolidate.rs`
- Modify: `app/src/main.rs` (moduł + podkomenda + usage)
- Test: `app/src/consolidate.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `pub fn consolidate(workspace: &Path, now: u64) -> Result<Report, String>`; `struct Report { pub facts: usize, pub episodes: usize, pub clusters: usize }`; plik `memory/graph.md` (nagłówek + sekcje tematyczne).

- [ ] **Step 1: Test — pusty workspace daje `Ok` i nie tworzy pliku**

```rust
#[test]
fn empty_workspace_writes_nothing() {
    let dir = setup("pusty");
    let report = consolidate(&dir, 1000).unwrap();
    assert_eq!(report.facts, 0);
    assert!(!dir.join("memory").join("graph.md").exists());
}
```

- [ ] **Step 2: Uruchom `cargo test --quiet consolidate` — FAIL (brak funkcji)**

- [ ] **Step 3: Test — fakty i epizody dają klastry tematyczne**

Wejście: `.agent-memory/facts.md` z 2 faktami o „pamiec" i 1 o „web"; `.agent-memory/episodes.md` z 2 wierszami.
Asercje: `report.facts == 3`, `report.episodes == 2`, `report.clusters == 2`, `graph.md` zawiera `## pamiec (2)` i `## web (1)`.

- [ ] **Step 4: Implementacja `consolidate`**

Czyta oba pliki, bierze linie zaczynające się od `- `, liczy klastry po pierwszym rzeczowniku z listy tematów (`pamiec`, `web`, `narzedzia`, `bezpieczenstwo`, `inne`), zapisuje `memory/graph.md` posortowane po liczbie malejąco. Wynik w `Result<Report, String>`; brak plików = zera, nie błąd.

- [ ] **Step 5: Podkomenda**

`ardntai consolidate` → wypisuje `fakty N, epizody M, klastry K` i ścieżkę pliku; exit 0.

- [ ] **Step 6: `cargo test --quiet` + `make verify` + commit**

---

### Task 2: Second brain — vault i narzędzie `memory.ingest`

**Files:**
- Create: `app/src/vault.rs`
- Modify: `app/src/tools.rs` (narzędzie `memory.ingest`), `app/src/main.rs` (`ingest`), `docs/contracts/mcp.md` (nowe narzędzie)
- Test: `app/src/vault.rs`, `app/src/tools.rs`

**Interfaces:**
- Produces: `pub fn ingest(workspace: &Path, rel_source: &str, title: &str) -> Result<String, String>` — kopiuje tekst do `.agent-memory/vault/<slug>.md`, dopisuje wpis do `index.md`, zwraca ścieżkę względną.
- Tool: `memory.ingest` z args `{"path": "...", "title": "..."}`; sukces `{"ok": true, "output": {"saved": ".agent-memory/vault/x.md"}}`.

- [ ] **Step 1: Test — ingest kopiuje tekst i dopisuje index**

```rust
#[test]
fn ingest_copies_a_workspace_file_and_indexes_it() {
    let dir = setup("vault");
    std::fs::write(dir.join("nota.md"), "# Nota\n\ntresc").unwrap();
    let saved = ingest(&dir, "nota.md", "Nota").unwrap();
    assert_eq!(saved, ".agent-memory/vault/nota.md");
    let index = std::fs::read_to_string(dir.join(".agent-memory/vault/index.md")).unwrap();
    assert!(index.contains("[Nota](nota.md)"));
}
```

- [ ] **Step 2: Test — źródło spoza workspace jest odrzucone**

`ingest(&dir, "../etc/passwd", "x")` → `Err` z komunikatem zawierającym `poza workspace`; plik nie powstaje.

- [ ] **Step 3: Implementacja `vault::ingest`** (walidacja ścieżki przez `tools::resolve`, slug z tytułu: małe litery, spacje → `-`, tylko `[a-z0-9-]`)

- [ ] **Step 4: Narzędzie `memory.ingest`** w `tools.rs` (`NAMES`, `input_schema`, `call`) + test w `tools::tests`, że `..` daje `DENIED` i nie tworzy pliku.

- [ ] **Step 5: `ardntai ingest <plik> [--title "..."]`** w CLI + wpis w usage; test w `tests/cli.rs`: brak argumentu → exit 2 i komunikat.

- [ ] **Step 6: `docs/contracts/mcp.md` — nowa pozycja w tabeli narzędzi (v2 kontraktu nie zmienia się: to nowe narzędzie, nie zmiana formatu), `make verify`, commit**

---

### Task 3: Sesje robocze `worker-*` i `tick --run`

**Files:**
- Modify: `app/src/proactive.rs` (pole `action`, uruchamianie), `app/src/main.rs` (`tick --run`)
- Test: `app/src/proactive.rs`, `app/tests/cli.rs`

**Interfaces:**
- `Policy.action: Option<String>` — treść zadania do uruchomienia.
- `pub fn tick_run(workspace: &Path, now: u64, binary: &Path) -> Vec<Hit>` — jak `tick`, ale dla trafień z `action` uruchamia `<binary> run -w <ws> --resume <task>` w tle z `ARNDTAI_WORKER=<policy>`; nie czeka.
- Sesje robocze: `context::Session::start` dostaje id `worker-<policy>-<ts>` gdy `ARNDTAI_WORKER` jest ustawione.

- [ ] **Step 1: Test — `tick --run` bez `action` nie uruchamia procesu**

Policy bez `action` → `hits.len() == 1`, zero nowych sesji w `.agent/sessions`.

- [ ] **Step 2: Test — `tick --run` z `action` uruchamia jedno zadanie i nie blokuje**

Policy z `"action": "write w.txt :: worker"` → po `tick_run` w workspace powstaje sesja z prefiksem `worker-`, a `w.txt` istnieje po ≤ 5 s.

- [ ] **Step 3: Implementacja** `action` w loaderze, `ARNDTAI_WORKER` w `context::Session::start`, `tick_run` spawnuje proces i loguje `{"effect": "worker: <policy>"}`.

- [ ] **Step 4: `ardntai tick --run` w CLI; bez flagi zachowanie bez zmian**

- [ ] **Step 5: Test ograniczenia równoległości** — przy istniejącym `.agent/run.lock` `tick_run` nie startuje procesu i loguje „zajete".

- [ ] **Step 6: `make verify`, commit**

---

### Task 4: Self-improvement — propozycje bez auto-apply

**Files:**
- Create: `app/src/improve.rs`
- Modify: `app/src/main.rs` (`improve`, `improve --apply`)
- Test: `app/src/improve.rs`

**Interfaces:**
- `pub fn propose(workspace: &Path) -> Vec<Proposal>`; `struct Proposal { pub kind: String, pub title: String, pub body: String }`.
- `pub fn apply(workspace: &Path, proposal: &Proposal) -> Result<PathBuf, String>` — zapisuje tylko do `policies/` i tylko JSON walidowany tym samym loaderem.
- CLI: `ardntai improve` wypisuje propozycje; `--apply` zapisuje je do `docs/proposals/<ts>-<kind>.md` (nie zmienia artefaktów).

- [ ] **Step 1: Test — brak transkryptów daje pustą listę**

- [ ] **Step 2: Test — powtarzające się odmowy dają propozycję reguły**

Wejście: 3 sesje z `denied` (reason `scope`) → `propose` zwraca 1 propozycję `kind == "policy"`, `body` zawiera `"when": "denied"` i `"threshold": 3`.

- [ ] **Step 3: Test — `--apply` nie dotyka `policies/`**

Po `apply` plik w `docs/proposals/`, `policies/` bez zmian (porównanie listingu).

- [ ] **Step 4: Implementacja + CLI + usage**

- [ ] **Step 5: `make verify`, commit**

---

### Task 5: Decyzje o zakresie (ADR) i notatki REA

**Files:**
- Create: `docs/architecture/adr/ADR-004-plugins.md`, `ADR-005-self-hosted-ci.md`, `docs/re/README.md` (wpisy o brakujących rozbiórkach)
- Modify: `docs/STATUS.md` (tabela „świadomie poza zakresem")

- [ ] **Step 1: ADR-004** — pluginy in-process: poza zakresem v1, uzasadnienie (MCP pokrywa perimetr, pluginy to drugi format uprawnień), warunek powrotu (potrzeba >1 języka narzędzi).
- [ ] **Step 2: ADR-005** — CI: hosted nieaktywne (billing), bramką hooki lokalne + `make ci`; warunek powrotu: uregulowany billing albo repo publiczne.
- [ ] **Step 3: `docs/re/README.md`** — lista rozbiórek oczekiwanych przez spec (HyAtlas L1–L8, Hello Minds, SillyTavern, dsh, Hermes skills) z oznaczeniem „nie zrobione" i powodem (brak dostępu do źródeł).
- [ ] **Step 4: `docs/STATUS.md`** — tabela: pozycja specu → status (`DONE`/`CZĘŚĆ`/`POZA ZAKRESEM (ADR)`/`OTWARTE`).
- [ ] **Step 5: `make verify`, commit**

---

### Task 6: Domknięcie produktu

**Files:**
- Modify: `README.md`, `GATES.md`, `docs/STATUS.md`, `BIG-PICTURE.md`

- [ ] **Step 1: `README.md`** — sekcja „Co działa" z listą podkomend (`run`, `tui`, `demo`, `skills`, `status`, `tick`, `consolidate`, `ingest`, `improve`, `tools`) i sekcja „Czego nie ma" z odsyłaczem do ADR.
- [ ] **Step 2: `GATES.md`** — G27 (consolidate), G28 (vault+ingest), G29 (worker), G30 (improve), każda z komendą i oczekiwanym wynikiem.
- [ ] **Step 3: `BIG-PICTURE.md` + `docs/STATUS.md`** — aktualny stan, żeby dokument nie kłamał.
- [ ] **Step 4: Pełny przebieg:** `cargo test`, `demo`, `make verify`, push, `gh api .pushed_at` jako dowód.
