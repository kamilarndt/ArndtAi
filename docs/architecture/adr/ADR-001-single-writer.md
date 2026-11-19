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
