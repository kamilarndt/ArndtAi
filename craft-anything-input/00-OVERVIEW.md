# 00 — Overview: Hybrid Companion Agent

**Cel:** Zbudować całkowicie osobnego agenta-companiona współpracownika (nie plugin Hermesa, nie klon).

**Metoda:** Często rozbieraj istniejące systemy (Hermes, dsh-superagent, HyAtlas-Memory, SillyTavern, Hello Minds, inne) za pomocą narzędzi reverse-engineering. Bierz z nich sprawdzone wzorce, pisz własny kod.

**Priorytety v1:**
- Companion personality (SillyTavern-like)
- Pamięć Lite / Pro / Ultra (HyAtlas-style) + pamięć projektowa (folder w projekcie)
- Loop z bramkami
- Proaktywność (default-deny)
- Self-improvement (tylko deklaratywne)
- Skills (foldery)
- Pluginy in-process + MCP perimeter
- Second brain (pusty na start, ingest jawny)

**Nie rób:**
- Pluginu do Hermesa
- Kopiowania kodu 1:1
- Automatycznego przyjmowania książek
- Doradców (odłożone)

**Dokumenty decyzyjne (przed kodem M0):**
- `docs/contracts/mcp.md` — kontrakt MCP (3 narzędzia, schematy, błędy, sandbox)
- `docs/architecture/adr/ADR-001-single-writer.md`
- `docs/architecture/adr/ADR-002-gates-and-recovery.md`
- `docs/architecture/adr/ADR-003-sandbox-and-outbox.md`
1. **Hermes Agent** (Nous Research) — learning loop, skills, MEMORY.md, cron, subagents
2. **OpenClaw** — gateway, multi-channel, automations, local-first
3. **Kilo Code** — multi-mode agents, marketplace skills/MCP
4. **pi** — minimalny core + TypeScript extensions
5. Hello Minds (always-on companion, Skills jako playbooki, Circles, proaktywność)
6. HyAtlas-Memory (Lite/Pro/Ultra)
7. dsh-superagent (single-writer, outbox, proaktywność)
8. SillyTavern (format karty — technika)
