# ArndtAi

Agent wykonawczy z pętlą decyzyjną, bramkami (allowlista, budżet), perimetrem
narzędzi MCP i trwałym stanem w SQLite. Rdzeń w Rust, bez zależności HTTP
(provider woła `curl`).

Stan: M0 + Faza B zamknięte. Faza C (web UI) nie rozpoczęta — kontrakt danych
jest gotowy.

## Układ

```
app/                     kod (Cargo.toml, src/, tests/)
docs/contracts/          mcp.md, events.md — formaty zamrożone
docs/architecture/adr/   ADR-001..003
docs/re/                 notatki decyzyjne, odstępstwa
docs/ARCHITECTURE.md     jak zbudowane (moduły, pętla, magazyn)
docs/STATUS.md           co zrobione, co otwarte, dług
docs/README.md           mapa dokumentacji
craft-anything-input/    materiał wejściowy (spec 00-09), nie kod
audyty/                  audyt 2026-11-13 + paczka zip do audytu
scripts/verify.py        dowody do checks/craft.json (wymaga craft.json; patrz docs/STATUS.md)
GATES.md                 bramki akceptacji i komendy
AGENTS.md                zasady pracy w repo
```

Kod jest w `app/`, bo projekt jest prowadzony przez Craft Anything w trybie
„start-from": korzeń trzyma materiał, aplikacja żyje w `app/`.

## Uruchomienie

```sh
cd app
cargo build
cargo test
cargo run -- demo                       # 39 sprawdzen na realnych artefaktach
cargo run -- -w /tmp/ws "write a.txt :: tresc; read a.txt"
cargo run -- -w /tmp/ws "notify gotowe"
cargo run -- -w /tmp/ws --resume        # wznowienie po przerwaniu
```

Flagi: `-w/--workspace`, `--provider mock|anthropic|openai`, `--steps N`,
`--memory lite|pro|ultra`, `--mcp|--no-mcp`, `--resume`, `-q`.
Komendy pomocnicze: `mcp` (serwer MCP na stdin/stdout), `skills`, `status`,
`demo`.

## Bramki

Zadanie przechodzi, gdy: allowlista i budżet przepuszczają akcję, a po
wykonaniu krok jest `done` w `.agent/state.db`. Odmowa kończy się wpisem
`denied` i po trzech z rzędu statusem `FAILED_STALLED`.

## Stan trwały

- `.agent/state.db` — `sessions`, `steps`, `outbox`; WAL + `busy_timeout=5000`.
- `.agent/sessions/<id>.jsonl` — transkrypt eventów (`docs/contracts/events.md`).
- `.agent/resume` — `session_id`, w który trafia `--resume`.
- `.agent/outbox/notifications.log` — eksport powiadomień (źródłem prawdy jest
  tabela `outbox`).

## Bezpieczeństwo

- Ścieżki wyłącznie względne, bez `..`; cel jest canonikalizowany i porównywany
  z korzeniem **oraz** z `.agent` — symlink/alias do `.agent` jest odrzucany.
- Zapis atomowy (`create_new` + `O_NOFOLLOW` + `rename`), limit 1 MB.
- Klucz API nie występuje w `argv`: nagłówki idą do pliku `0600` przez
  `curl --config`.

## Dokumenty

- `AGENTS.md` — zasady pracy agenta w tym repo (układ, komendy, ograniczenia).
- `docs/README.md` — mapa całej dokumentacji.
- `docs/STATUS.md` — stan: co działa, co zamknięte po audycie, co otwarte.
- `docs/ARCHITECTURE.md` — budowa: moduły, pętla, magazyn, sandbox, provider.
- `docs/contracts/mcp.md` — koperta wywołań, kody błędów.
- `docs/contracts/events.md` — schemat eventów i tabel.
- `docs/re/README.md` — decyzje, odstępstwa, znane braki.
- `audyty/arndtai-2026-11-13.md` — audyt wejściowy (F1-F32).
