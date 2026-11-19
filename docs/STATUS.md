# Stan projektu — 2026-11-19 14:20 (poprzednio 2026-11-14 03:20)

## Jednym zdaniem

Rdzeń (M0 + Faza B), pełny TUI w zakresie bez runtime'u Groka (rundy 14–18)
i testy na żywo są zamknięte. Zostaje M1: web UI, naprawa perimetru MCP
po SIGKILL, Python core (ADR-004).

## Co działa (z dowodem)

| obszar | dowód |
|---|---|
| pętla + bramki | `cargo test` → **232 unit + 21 integration**, exit 0; `cargo run -- demo` → 39/39 |
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
| zrzut `--screenshot` | dwa testy w `app/tests/cli.rs` (PNG 960×608, rozmiar `83x38`, brak ścieżki → exit 2) — dodane przez CA w rundzie 19 |
| kill -9 | `app/tests/cli.rs` — trzy testy na prawdziwym SIGKILL: krok `done` wraca z wyniku, `proposed` → `abandoned`, perimetr MCP też nie powtarza; G15 w `GATES.md` |
| backfill FTS | baza bez `user_version` po migracji trafia w wyszukiwaniu — `store::tests::backfill_indexes_transcripts_written_before_fts_existed` (dowód: po usunięciu `rebuild` test jest czerwony) |
| kontrakty | `mcp.md` v0.2 (+`fs.append`), `events.md` (+`repair`, tabele M1) |
| bramki | `GATES.md` G1–G13 z hashami |

## Co pozostaje (M1)

1. ~~Faza C — web UI (MVP)~~ — `app/web/` (HTML+JS, bez zależności): lista sesji,
   12 typów eventów, filtr `kind`, szukanie, tail po pełnych liniach; sprawdzone
   w Orca browser (commit `d52a675`, zrzut `app/shots/faza-c-sesja.png`). Zostaje
   przeniesienie renderu markdown z TUI i sterowanie zadaniami (poza MVP).
2. ~~Perimetr MCP po SIGKILL~~ — sprawdzone 19 listopada: perimetr oddaje wynik
   z `steps.result_json`, efektu nie powtarza; trzy testy w `app/tests/cli.rs`.
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


## v0.1 (2026-11-19)

- `cargo install --path app` → `ardntai 0.1.0`; `--version` to flaga (G17).
- Realne zadanie na żywym modelu (`mimo-v2.6-flash`, OC-Go): DONE w 3 krokach,
  2 wywołania narzędzi, plik `one.txt` = `alfa`; transkrypt
  `app/shots/live-v01b-sesja.jsonl`.
- README przepisany pod użytkownika (instalacja, pierwsze zadanie, ograniczenia).
- ADR-004 zamknięty: rdzeń zostaje w Rust.

## Audyt ustaleń (2026-11-19)

`docs/audits/spec-gap-2026-11-19.md` — 79 wymagań z `craft-anything-input/00-09`:
DONE 36, CZĘŚĆ 24, BRAK 18. Domknięte tego dnia:

- kontrakt eventów **v2** z `gate` (G18) — trajectory ma etapy bramek;
- `personality/base.md` i `skills/przyklad/SKILL.md` w repo (A08) — `ardntai skills`
  widzi osobowość z pliku i 1 skill (647 B, lista w promptcie: 70 znaków);
- single-writer: ADR-001 nowelizacja (A19) — perimetr MCP jako świadomy wyjątek
  ograniczony do tabeli `steps`.

Otwarte: web control (`POST /run`), pamięć Pro/Ultra, proaktywność.

## v0.3 (2026-11-19)

- `app/web/serve.py` — serwer UI (Python, stdlib): `POST /api/run`, `--resume`,
  `POST /api/stop`, `GET /api/status`, `GET /api/sessions`, tail po `Range`.
- UI: pole zadania + `Uruchom`/`Zatrzymaj`, status biegu, auto-wybór nowej sesji.
- Bezpieczeństwo: bind 127.0.0.1, jedno zadanie naraz (409), treść zadania nie
  jest ścieżką. Rdzeń (`app/src/`) i `Cargo.toml` nietknięte.
