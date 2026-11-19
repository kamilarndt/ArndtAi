# Architektura — ArndtAi

Opis budowy na 2026-11-13. Plik opisuje **stan faktyczny kodu**, nie plan.
Decyzje i odstępstwa: `docs/architecture/adr/`, `docs/re/README.md`.

## 1. Proces i transporty

Jeden binarny plik `ardntai` (`app/src/main.rs`) ma dwa tryby pracy:

- **rdzeń** — `ardntai run "zadanie"` (domyślnie także bez słowa `run`): pętla
  decyzyjna, bramki, składanie kontekstu, zapis stanu;
- **perimetr** — `ardntai mcp`: serwer MCP po stdio (JSON-RPC 2.0), jedyny
  wykonawca narzędzi `fs.*` i `notify.send`.

Rdzeń domyślnie spawnuje perimetr (`--mcp`); `--no-mcp` wykonuje narzędzia
w procesie. Oba warianty wołają ten sam `tools::call`, więc walidacja i sandbox
są wspólne. Komendy pomocnicze: `demo`, `tools`, `skills`, `status`,
`--screenshot <plik.png>`.

## 2. Pętla decyzyjna (`app/src/agent.rs`)

Jedno przejście = jeden krok:

```
budżet (max_steps) → decyzja modelu (Brain::next)
  → allowlista → scope guard (tools::resolve) → detektor dryfu (okno 4 wywołań)
  → proposed (SQLite) → wykonanie (MCP albo in-process)
  → done + result_json (+ outbox w tej samej transakcji dla notify.send)
  → observe → stop_check
```

- Stany emitowane jako eventy: `RECEIVED`, `PLANNING`, `STEP_PROPOSED`,
  `POLICY_CHECK`, `SCOPE_CHECK`, `EXECUTING`, `OBSERVE`, `STOP_CHECK`, `DONE`,
  `FAILED_*` (`docs/contracts/events.md`).
- Trzy odmowy z rzędu → `FAILED_STALLED`; wyczerpany budżet → `FAILED_BUDGET`.
- Bramki: **allowlista**, **scope guard**, **dryf** (odcisk `tool+args`
  powtórzony w oknie 4 wywołań, w tym wzorzec A,B,A,B) oraz **budżet** sprawdzany
  na początku kroku. ADR-002 opisuje M0 jako dwie bramki; kod ma trzy plus
  budżet — różnica jest zapisana w `docs/STATUS.md`.

## 3. Stan trwały (`app/src/store.rs`)

SQLite `.agent/state.db`, `PRAGMA journal_mode=WAL`, `busy_timeout=5000`:

| tabela | rola |
|---|---|
| `sessions` | `id`, `task`, `started_at` — jedna sesja = jeden plik JSONL |
| `steps` | `(session_id, step_id)` PK; `status` ∈ `proposed`/`done`/`abandoned` |
| `outbox` | payload powiadomienia, unikalny per krok (`outbox_step_once`) |

Reguły: `proposed` **przed** efektem, `done` **po** nim; wznowienie czyta `done`
i nie wykonuje ponownie; `proposed` po crashu → `abandoned` (at-most-once).
Przy transporcie MCP kroki zapisuje serwer perimetru na tym samym `state.db`
(rdzeń nie dubluje `proposed`).

Transkrypty: `.agent/sessions/<id>.jsonl` — log eventów, nie źródło prawdy.
Wskaźnik wznowienia: `.agent/resume` (`--resume`).

## 4. Perimetr MCP (`app/src/mcp.rs`)

Metody: `initialize`, `notifications/initialized`, `ping`, `tools/list`,
`tools/call`, `shutdown`. Koperta `tools/call` wymaga `task_id`, `session_id`,
`step_id`; ten sam `step_id` z innymi argumentami jest odrzucany, a retry zwraca
zapisany wynik (zero drugiego efektu). Kontrakt: `docs/contracts/mcp.md`.

## 5. Narzędzia i sandbox (`app/src/tools.rs`)

Trzy narzędzia (`NAMES`): `fs.read`, `fs.write`, `notify.send`. Schematy
argumentów: `app/src/schema.rs` (walidacja przed dotknięciem dysku).

- Ścieżki tylko względne; `..` i ścieżki bezwzględne odrzucane.
- Cel jest canonicalizowany i porównywany z korzeniem **oraz** z `.agent` —
  alias/symlink do `.agent` jest odrzucany.
- `fs.write`: atomowo (`create_new` + `O_NOFOLLOW` + `rename`), limit 1 MB.
- `fs.read`: limit 4 MB.
- `notify.send`: wiersz w `outbox` w transakcji kroku + eksport do
  `.agent/outbox/notifications.log` (eksport, nie źródło).

## 6. Kontekst i pamięć (`app/src/context.rs`)

Prefiks systemowy = `personality/base.md` + wybrane pliki pamięci projektu.
Poziomy:

| poziom | pliki |
|---|---|
| `lite` | `memory/project.md` (także `.agent-memory/`, `.agent/memory/`) |
| `pro` | + `facts.md`, `episodes.md` |
| `ultra` | + `graph.md` |

Brak plików nie jest błędem — prefiks powstaje z tego, co istnieje.

## 7. Provider (`app/src/provider.rs`)

- `mock` — deterministyczny plan offline, bez sieci; używany przez `demo` i testy.
- `anthropic` / `openai` — przez `curl`, klucz z `ANTHROPIC_API_KEY` /
  `OPENAI_API_KEY`, opcjonalnie `ARNDTAI_MODEL`, `ARNDTAI_BASE_URL`.
- Nagłówki z kluczem trafiają do pliku `0600` podawanego curlowi przez
  `--config`; w `argv` zostaje tylko nazwa pliku.

## 8. Mapa modułów

| plik | linie | odpowiedzialność |
|---|---|---|
| `app/src/main.rs` | 701 | CLI, komendy, `demo`, `--screenshot` |
| `app/src/agent.rs` | 643 | pętla, bramki, wznowienie |
| `app/src/tools.rs` | 762 | narzędzia, sandbox, zapis atomowy |
| `app/src/mcp.rs` | 611 | serwer i klient MCP |
| `app/src/provider.rs` | 557 | mock + Anthropic/OpenAI przez curl |
| `app/src/store.rs` | 357 | SQLite, transakcje, outbox |
| `app/src/context.rs` | 290 | prefiks, pamięć, sesja/eventy |
| `app/src/shot.rs` | 258 | zrzut PNG |
| `app/src/schema.rs` | 152 | walidacja JSON Schema |

## 9. Testy

`cargo test` = 34 testy jednostkowe (w modułach) + 4 integracyjne
(`app/tests/cli.rs`). `cargo run -- demo` to 39 sprawdzeń end-to-end na realnych
artefaktach (pętla, scope, allowlista, MCP, kontrakt, dryf, pamięć).
