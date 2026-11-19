# ADR-004: Rdzen Rust w M0, Python core od M1

**Status:** Proposed  
**Data:** 2026-11-13  
**Powiazane:** F17, ADR-001, ADR-002, ADR-003

## Problem
Caly agent jest w Rust (petla, bramki, MCP, SQLite, TUI). Oryginalny plan zakladal Python core + perimeter w innym jezyku dla ekosystemu (pamiec, skille, proaktywnosc, self-improvement).

## Decyzja
**Na M0 zostajemy przy rdzeniu w Rust.**  
Migracja do Python core + Rust perimeter jest decyzja M1, nie poprawka M0.

## Uzasadnienie
- M0 + Faza B zamkniete: 40 testow unit + 4 integracyjne, GATES G1-G3 z hashami.
- TUI (ratatui) dziala i konsumuje `docs/contracts/events.md`.
- Kontrakty sa jezykowo neutralne — migracja nie wymaga ich zmiany.
- Koszt migracji teraz > wartosc (brak brakujacych funkcji, ktore Python by odblokowal w M0).

## Konsekwencje
- Rdzen Rust zostaje zrodlem prawdy do czasu jawnej decyzji M1.
- TUI zostaje w Rust (klient kontraktu, nie czesc rdzenia).
- Przy migracji M1: Python = jedyny writer SQLite; Rust = tylko MCP server; recovery semantyka bez zmian.

## Alternatywy odrzucone
- Migracja natychmiastowa: wysoki koszt, zerowy zysk funkcjonalny przy zamknietym M0.
- Rdzen na zawsze w Rust: zamyka drzwi do ekosystemu Python dla pamieci/skilli, ale nie jest problemem M0.
