# Dokumentacja ArndtAi — mapa

Kolejność czytania: `README.md` (korzeń) → ten plik → `STATUS.md` →
`ARCHITECTURE.md` → kontrakty i ADR.

| dokument | po co | status |
|---|---|---|
| `../README.md` | czym jest projekt, jak uruchomić, gwarancje | żywy |
| `../AGENTS.md` | zasady pracy agenta w tym repo, układ katalogów | żywy |
| `../GATES.md` | bramki akceptacji G1-G11 + komendy i dowody | żywy (dopisywany) |
| `STATUS.md` | co działa, co zamknięte po audycie, co otwarte | żywy (aktualizuj przy zmianie stanu) |
| `ARCHITECTURE.md` | moduły, pętla, magazyn, sandbox, provider | żywy (opis stanu faktycznego) |
| `contracts/mcp.md` | koperta wywołań MCP, kody błędów, timeouty | **zamrożony v0.1** |
| `contracts/events.md` | typy eventów JSONL i tabele SQLite | **zamrożony v1** |
| `architecture/adr/ADR-001-single-writer.md` | kto pisze do SQLite | decyzja |
| `architecture/adr/ADR-002-gates-and-recovery.md` | bramki M0 i recovery | decyzja (zakres M0) |
| `architecture/adr/ADR-003-sandbox-and-outbox.md` | sandbox i outbox | decyzja |
| `re/README.md` | odstępstwa od spec, dług, wybory po audycie | żywy |

Poza `docs/`:

| ścieżka | rola |
|---|---|
| `audyty/arndtai-2026-11-13.md` | audyt wejściowy (32 znaleziska) |
| `audyty/ardntai-agent-2026-11-13.zip` | paczka agenta do audytu (kod + binarka + hashe) |
| `craft-anything-input/00-09*.md` | wymagania wejściowe (spec), nie dokumentacja produktu |
| `scripts/verify.py` | generuje dowody do `checks/` i `craft.json` |

Zasada: **formaty zamrożone** zmienia się przez nową wersję pliku i wpis
w `STATUS.md`, nigdy cichą edycją.
