# 02 — Memory (Lite / Pro / Ultra + Project)

Wzorzec: HyAtlas-Memory.

| Poziom | Co robi | Kiedy |
|--------|---------|-------|
| **Lite** | Tylko raw + search. Zero LLM. | Research, jednorazowe |
| **Pro** | + ekstrakcja faktów, epizody, Now page | Większość zadań |
| **Ultra** | + konsolidacja w tle (graf, schematy, intencje, tożsamość) | Codzienny companion |

**Pamięć projektowa:** folder w projekcie (np. `.agent-memory/` lub `memory/`). Izolowana. Ładowana gdy agent pracuje nad tym projektem.

**Second brain:** pusty na start. Ingest tylko na jawne polecenie / przez narzędzie. Aktywny głównie na Ultra.

**Selektywność:** runtime wstrzykuje tylko to, co potrzebne do bieżącej decyzji.

## Reverse-engineering mandate
Rozbierz HyAtlas-Memory (warstwy L1–L8, tryby lite/pro/ultra, konsolidacja).  
Weź model warstw i trybów, zaimplementuj własny.
