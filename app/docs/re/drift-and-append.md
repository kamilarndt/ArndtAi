# Dryf nie dławi odczytów + `fs.append` (runda 9)

Data: 2026-11-13. Zakres: `app/src/agent.rs`, `app/src/tools.rs`,
`app/src/provider.rs`, etykiety narzędzi w TUI (`tui.rs`, `sessions.rs`),
demo. Kontrakty, web UI, recovery i SQLite bez zmian.

## 1. Dryf liczy tylko mutacje

**Było:** odcisk `(tool+args)` trafiał do okna czterech wywołań dla każdego
narzędzia. Sekwencja `fs.read a.txt → fs.write a.txt → fs.read a.txt` dawała
dwie odmowy dryfu z rzędu (trzecia, np. kolejny odczyt, kończyła sesję
`FAILED_STALLED`), choć odczyt nic nie zmienia.

**Jest:** okno dryfu wypełniają wyłącznie narzędzia zmieniające stan —
`fs.write`, `fs.append`, `notify.send` (`tools::is_mutating`). `fs.read` nie
jest dodawany do okna i nie może wywołać odmowy. Odmowa dla odczytu zostaje
tylko wtedy, gdy da ją scope guard (ścieżka poza workspace, `..`, `.agent`,
symlink na zewnątrz).

Bez zmian: próg trzech odmów z rzędu kończący sesję, licznik `drift`
w statusie i w evencie `denied`, kolejność bramek (allowlista → scope guard →
dryf). Zmiana jest w `agent.rs` przy budowie `is_drift` i przy dopisywaniu
odcisku do `recent`.

## 2. Nowe narzędzie `fs.append`

- Argumenty jak `fs.write`: `path`, `content`; wynik `{bytes_written}`.
- Dopisuje na końcu pliku i tworzy go, jeśli nie istnieje; tworzy też katalogi
  po drodze, jak `fs.write`.
- Te same bramki: allowlista, scope guard (bez `..`, bez wyjścia poza
  workspace, `.agent` zarezerwowany), symlink kończący ścieżkę odrzucony
  (`O_NOFOLLOW`), a istniejący cel musi być zwykłym plikiem.
- Limity treści te same co `fs.write`: 1 MB to `DENIED`, 64 KB to
  `SCHEMA_ERROR` — wspólna funkcja `content_arg`.
- `NAMES`, `describe()`, `input_schema()` i `tools/list` wystawiają cztery
  narzędzia: `fs.read`, `fs.write`, `fs.append`, `notify.send`.
- Mock offline rozumie `append <ścieżka> :: <treść>` (i `dopisz`), tak jak
  `write`; prompt zdalnego providera wymienia `fs.append`.

## 3. Dowody

- `cargo build` — exit 0.
- `cargo test` — 98 testów jednostkowych i 4 integracyjne, wszystkie zielone.
  Nowe: `read_write_read_has_no_drift`,
  `drift_detects_alternating_mutation_in_window`,
  `repeated_write_is_still_drift`,
  `append_twice_drifts_but_append_then_read_does_not`,
  `append_outside_workspace_is_denied_and_creates_nothing`,
  `append_creates_file_and_reports_bytes`,
  `append_through_alias_to_agent_is_denied`,
  `append_is_exported_like_write`.
- `cargo run -- demo` — 39/39. Blok dryfu używa teraz dwóch identycznych
  `fs.write` (odczyt nie jest już dryfem).
- `cargo run -- --screenshot app/shots/now.png` — PNG 960×608, ekran sesji
  bez zmian w układzie.

## 4. Czego świadomie nie zmieniono

- `docs/contracts/mcp.md` i `events.md` — nowa wersja `mcp.md` (czwarte
  narzędzie) jest zapowiedziana po tej rundzie; do tego czasu kontrakt
  zostaje na v0.1, a kod wystawia `fs.append` zgodnie z opisem zadania.
- Progu `FAILED_STALLED` i znaczenia `dryf` w statusie.
- TUI: `fs.append` ma tylko etykiety `Appending`/`Appended` i ścieżkę jako
  temat; blok diffu nadal rysuje wyłącznie `fs.write` (dopisanie nie jest
  nową treścią pliku).
- Web UI, recovery, SQLite, outbox — nietknięte.
