# 01 — Architecture

## Procesy
- **Rdzeń** (Python na start): pętla, bramki, składanie kontekstu, single-writer, wybór poziomu pamięci, ładowanie pamięci projektowej.
- **Perimeter** (TypeScript lub Rust): serwer MCP. Jedyny dostęp do świata zewnętrznego, sekrety, triggery, sandbox.
- **Pluginy**: in-process, obok MCP (dla rzeczy bliskich rdzeniowi).

## Kanał
- MCP po stdio na start (rdzeń spawnuje perimeter).
- UDS/HTTP jako opcja później.

## Zasady
- Rdzeń nie ma sekretów ani bezpośredniej sieci (poza providerem LLM).
- Single-writer: rdzeń pisze agregaty zadaniowe. Perimeter pisze tylko surowe zdarzenia do inboxu.
- Outbox na efekty uboczne (wzorzec z dsh).
- Prefiks stabilny w obrębie zadania (KV-cache).

## Reverse-engineering mandate
Rozbierz:
- Hermes `tools/mcp_tool.py` i lifecycle stdio.
- dsh-superagent Engine / outbox / single-writer.
- Porównaj i wybierz najlepsze elementy do własnej implementacji.
