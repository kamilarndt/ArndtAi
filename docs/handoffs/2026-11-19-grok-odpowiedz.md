# Odpowiedź Groka — ArndtAi, 2026-11-19

Wysłane przez Orca browser do czatu „Architektura agenta Argos — rdzeń Python i obwód
TypeScript" (Grok, model Szybki). Poniżej streszczenie z zachowaniem decyzji; pełna
odpowiedź jest w tym czacie.

## Ocena

Projekt „w dobrym miejscu". M0 + Faza B + TUI realnie zamknięte: 232+15 testów,
demo 39/39, verify OK, live 8/8, FTS5 z backfillem. Słabe punkty są operacyjne,
nie architektoniczne: brak gita, `live_test.py` nieodpalany od 14.11, duże pliki
(`tui.rs` 280 KB).

## Decyzje

1. **Git pierwsze.** „Bez historii każda kolejna runda jest nieodwracalna. Kill -9
   to pokrycie semantyki, którą już macie (at-most-once). Utrata kodu jest gorsza
   niż brak jednego testu odporności."
2. **TUI zostaje w Rust**, nie przenosić. **Faza C tak, ale po gicie** — „drugi
   klient (web) to najlepszy sposób na weryfikację kontraktu eventów".
3. **Rdzeń zostaje w Rust**; ADR-004 zostaje `Proposed`. Python core dopiero gdy
   pojawi się konkretna potrzeba ekosystemu (pamięć Pro, skille, proaktywność),
   której nie da się rozsądnie zrobić w Rust — dziś takiej nie ma.
4. **Parzystość z grok-build: wystarczy funkcjonalnie równoważny TUI.**
   „Pixel-perfect nie jest celem produktu."

## Kolejność (tabela Groka)

| # | Co | Dlaczego |
|---|---|---|
| 1 | Git init + pierwszy commit (bez `target/`) | chroni pracę |
| 2 | `.gitignore` + `scripts/verify.py` w CI-lokalnym | stabilne dowody |
| 3 | Testy kill -9 (opcjonalnie równolegle) | domknięcie odporności |
| 4 | Faza C — minimalny web UI na JSONL | drugi klient kontraktu |
| 5 | M1 dane (tasks/transcripts, jeśli jeszcze niepełne) | fundament pod companion |

Pamięć Pro / skille / Python core — później, przy konkretnym use-case companiona.

## Jednym zdaniem Groka

„Zróbcie repozytorium, zamknijcie kill -9 jeśli macie czas, potem web UI jako drugi
klient eventów. Nie migrujcie rdzenia i nie gońcie już grok-builda."

## Rozbieżności z moją rekomendacją

- Grok zgadza się co do gita i co do Rust. Różni się w jednym: on wstawia
  `verify.py` do lokalnego CI jako krok 2, ja tego nie proponowałem — to tanie
  i usuwa problem „assert czerwony po każdej zmianie w `src/`".
- Grok nazywa M1-dane krokiem 5 („jeśli jeszcze niepełne"); w repo `tasks`
  i `transcripts` są od rundy 11 i mają testy, więc ten krok jest prawdopodobnie
  pusty — do sprawdzenia przed wzięciem.
