# ADR-003: Sandbox fs.write i outbox dla notify.send

**Status:** Accepted  
**Data:** 2026-11-12

## Problem
`fs.write` bez ograniczeń ścieżek jest niebezpieczne. `notify.send` bez outbox może zostać utracone lub wysłane podwójnie przy crashu.

## Decyzja
- **fs.write** działa wyłącznie w skonfigurowanym `workspace_root`. Perimeter odrzuca path traversal, symlinki wychodzące i nadmierny rozmiar. Zapis atomowy.
- **notify.send** jest zapisywane przez rdzeń do tabeli `outbox` w tej samej transakcji co zmiana stanu. Worker outbox wykonuje wysyłkę po commit.

## Uzasadnienie
- Klasyczne wymaganie bezpieczeństwa filesystem.
- Wzorzec outbox z dsh-superagent i recenzji.
- Chroni przed utratą i podwójnym efektem.

## Konsekwencje
- Perimeter ma jawną logikę sandbox.
- Rdzeń ma prosty outbox worker.
