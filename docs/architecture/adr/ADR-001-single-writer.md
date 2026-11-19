# ADR-001: Single-writer w M0

**Status:** Accepted  
**Data:** 2026-11-12

## Problem
Perimeter i rdzeń mogą jednocześnie pisać do tej samej bazy SQLite (`trigger_inbox` vs agregaty). To grozi lockami i korupcją.

## Decyzja
W M0 **tylko rdzeń** pisze do SQLite.  
Perimeter nie ma własnej bazy. Surowe zdarzenia wracają do rdzenia przez MCP i są zapisywane przez rdzeń.

## Uzasadnienie
- Zgodne z zasadą single-writer z dokumentów.
- Prostsze i bezpieczniejsze na start.
- Potwierdzone w recenzjach (DeepSeek, Meta) i praktyce SQLite.

## Konsekwencje
- Proaktywność (trigger_inbox) jest poza M0 albo realizowana przez MCP callback do rdzenia.
- Łatwiejsze recovery i transakcje.


## Nowelizacja 2026-11-19: perimetr MCP jako drugi pisarz (świadomy wyjątek)

Audyt `docs/audits/spec-gap-2026-11-19.md` (A19) wskazał, że `app/src/mcp.rs`
otwiera tę samą `.agent/state.db` i zapisuje `proposed`/`complete` dla kroków
wykonywanych przez perimetr. To narusza zdanie „tylko rdzeń pisze".

**Decyzja:** zostaje, jako wyjątek ograniczony do jednej tabeli (`steps`) i jednej
transakcji na krok. Powody:

- bramka „czy ten krok już był wykonany" musi być sprawdzona **przed** efektem
  w procesie, który efekt wykonuje — inaczej okno między sprawdzeniem a zapisem
  rośnie o IPC;
- SQLite w WAL z `busy_timeout = 5000` obsługuje dwóch pisarzy na krótkich
  transakcjach; konflikt kończy się `store_error`, nie utratą danych;
- test `killing_the_process_inside_the_tool_leaves_the_step_abandoned`
  (`app/tests/cli.rs`) pokrywa scenariusz, w którym liczy się ta własność.

**Granice wyjątku:** perimetr nie pisze `sessions`, `tasks`, `transcripts`,
`outbox` ani `turns`; nie tworzy własnej bazy; nie wykonuje migracji.
Jeśli w przyszłości perimetr potrzebowałby więcej niż `steps`, wracamy do
projektu z jednym pisarzem (kolejka w rdzeniu zamiast wspólnej bazy).
