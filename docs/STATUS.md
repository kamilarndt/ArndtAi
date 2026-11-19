# Stan projektu — 2026-11-19 14:20 (poprzednio 2026-11-14 03:20)

## Jednym zdaniem

Rdzeń (M0 + Faza B), pełny TUI w zakresie bez runtime'u Groka (rundy 14–18)
i testy na żywo są zamknięte. Zostaje M1: web UI, naprawa perimetru MCP
po SIGKILL, Python core (ADR-004).

## Co działa (z dowodem)

| obszar | dowód |
|---|---|
| pętla + bramki | `cargo test` → **232 unit + 17 integration**, exit 0; `cargo run -- demo` → 39/39 |
| trwały stan, wznowienie | nowa tura liczy kroki dalej (`max+1`), replay tylko w turze |
| idempotencja, sandbox | `mcp::tests`, `tools::tests`, `store::tests` |
| narzędzia | 5: `fs.read`, `fs.write`, `fs.append`, `notify.send`, `skill.load` |
| TUI — ekrany | powitanie, rozmowa, lista sesji, modal sesji, `/model`, `/settings`, `/skills`, wyszukiwanie |
| TUI — dane | okno kontekstu z `limit.context` modelu; ceny z `cost.text.*`; licznik `skille N` |
| TUI — komendy | `/help`, `/model`, `/settings`, `/skills`, `/skill`, `/copy`, `/export`, `/clear`, `/quit`, `/resume` |
| ustawienia | `.agent/config`: motyw, tryb zgód (`always-approve`/`ask`), pamięć, znaczniki, mysz, limit odpowiedzi; env > plik > domyślna |
| skille | `skills/<nazwa>/SKILL.md`, trzy katalogi, lista w prompcie (max 20), treść przez `/skill` lub `skill.load` (8 KB) |
| markdown | nagłówki, listy, kod z podświetlaniem (rust/json/bash/toml), tabela, cytat; surowe w wynikach narzędzi |
| testy na żywo | `scripts/live_test.py`, 8/8 (realny model OC-Go, pty + pyte) |
| kill -9 | `app/tests/cli.rs` — dwa testy na prawdziwym SIGKILL: krok `done` wraca cached, `proposed` → `abandoned`; G15 w `GATES.md` |
| backfill FTS | baza bez `user_version` po migracji trafia w wyszukiwaniu — `store::tests::backfill_indexes_transcripts_written_before_fts_existed` (dowód: po usunięciu `rebuild` test jest czerwony) |
| kontrakty | `mcp.md` v0.2 (+`fs.append`), `events.md` (+`repair`, tabele M1) |
| bramki | `GATES.md` G1–G13 z hashami |

## Co pozostaje (M1)

1. Faza C — web UI (kontrakt eventów gotowy; render markdown gotowy do przeniesienia).
2. Perimetr MCP: po SIGKILL wznowienie wykonuje krok `done` drugi raz (znalezione
   19 listopada przy testach kill -9; ścieżka rdzenia `--no-mcp` jest poprawna).
3. Python core (ADR-004 Proposed).
4. **Repo git jest** (`github.com/kamilarndt/ArndtAi`, prywatne, od 19 listopada);
   `make verify` + hook pre-commit pilnują dowodów przed commitem.
5. Streaming tokenów: blok rośnie w miejscu, ale pętla dostaje jedną myśl na krok,
   więc efekt będzie widoczny po zmianie providera.

## Znane ograniczenia (nie błędy)

- Licznik kontekstu: `usage` używane, gdy provider je zwraca; inaczej szacunek z `~`.
- `/copy` zależy od terminala (OSC 52 bywa wyłączone) — komunikat mówi „wysłano".
- Oryginalnego pagera nie uruchamialiśmy (wymaga konta xAI); wierność oparta na
  zrzucie README i kodzie `xai-grok-pager`.
- Glif `❯` w zrzutach PNG wychodzi jako `>` — font8x8 nie ma U+276F; w terminalu jest poprawny.
