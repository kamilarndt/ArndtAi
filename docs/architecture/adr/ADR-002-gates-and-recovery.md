# ADR-002: Semantyka bramek i recovery w M0

**Status:** Accepted  
**Data:** 2026-11-12

## Problem
Dokumenty wymieniają 4 bramki, M0 mówi o 2. Brak decyzji co się dzieje przy naruszeniu i jak odzyskiwać stan po crashu.

## Decyzja
W M0 obowiązują **dwie bramki**:

- **Budget** (`max_steps`, `max_tool_calls`) → `FAILED_BUDGET`
- **Allowlist** narzędzi → `POLICY_DENIED`

Scope guard i drift detector są poza M0 (stuby).

Stany są zapisywane w transakcji. Po restarcie agent wznawia od ostatniego zatwierdzonego stanu i **nie powtarza** już wykonanego tool call (idempotencja przez `step_id`).

## Uzasadnienie
- Minimalny bezpieczny zakres M0.
- Zgodne z recenzjami i dokumentami.
- Umożliwia recovery bez podwójnych efektów.

## Konsekwencje
- Proste testy bramek.
- Jasne stany terminalne.
