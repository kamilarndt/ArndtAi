# MCP Contract — M0

**Wersja:** 0.2 (2026-11-13)  
**Transport:** stdio (JSON-RPC)  
**Właściciel cyklu życia:** rdzeń (spawn, healthcheck, shutdown)

## Zmiany

- **0.2 (2026-11-13):** dodano `fs.append`; doprecyzowano, że detektor dryfu
  dotyczy tylko narzędzi ze side-effectem (`fs.write`, `fs.append`,
  `notify.send`), a `fs.read` nie jest powtarzalnym dryfem.
- 0.1 (2026-11-12): pierwsza wersja — `fs.read`, `fs.write`, `notify.send`.

## Narzędzia M0

| Narzędzie | Opis | Side-effect |
|-----------|------|-------------|
| `fs.read` | Odczyt pliku z dozwolonego workspace | nie |
| `fs.write` | Zapis pliku w dozwolonym workspace | tak |
| `fs.append` | Dopisanie na końcu pliku w dozwolonym workspace | tak |
| `notify.send` | Wysłanie powiadomienia | tak |

## Wspólne zasady

- Każde wywołanie zawiera `task_id`, `session_id`, `step_id`.
- Timeout domyślny: 30 s (konfigurowalny per narzędzie).
- Perimeter waliduje argumenty i ścieżki przed wykonaniem.
- Błędy zwracane jako ustrukturyzowane JSON (nie tylko tekst).

## Error codes

| Kod | Znaczenie | Retry |
|-----|-----------|-------|
| `DENIED` | Odrzucone przez politykę / allowlist / sandbox | nie |
| `NOT_FOUND` | Plik nie istnieje | nie |
| `IO_ERROR` | Błąd I/O | tak (ograniczone) |
| `TIMEOUT` | Przekroczony czas | nie |
| `SCHEMA_ERROR` | Niepoprawne argumenty | nie |

## fs.read

**Input:**
```json
{
  "path": "string (relative to workspace)"
}
```

**Output:**
```json
{
  "content": "string",
  "size": "number"
}
```

## fs.write

**Input:**
```json
{
  "path": "string (relative to workspace)",
  "content": "string"
}
```

**Sandbox (obowiązkowy):**
- Tylko ścieżki wewnątrz skonfigurowanego `workspace_root`.
- Odrzucenie `..`, absolutnych ścieżek i symlinków wychodzących poza workspace.
- Limit rozmiaru treści (konfigurowalny, domyślnie 1 MB).
- Zapis atomowy (temp + rename).

**Output:**
```json
{
  "bytes_written": "number"
}
```

## fs.append

**Input:**
```json
{
  "path": "string (relative to workspace)",
  "content": "string"
}
```

Semantyka: dopisuje `content` na końcu pliku; tworzy plik, jeśli go nie ma.
Nigdy nie obcina istniejącej treści.

**Sandbox:** identyczny jak `fs.write` (wnętrze `workspace_root`, bez `..`,
bez ścieżek absolutnych, bez symlinków wychodzących na zewnątrz, limit 1 MB).

**Output:**
```json
{
  "bytes_written": "number"
}
```

`bytes_written` to liczba **dopisanych** bajtów, nie rozmiar pliku.

## notify.send

**Input:**
```json
{
  "channel": "string",
  "message": "string"
}
```

**Output:**
```json
{
  "status": "queued | sent"
}
```

W M0 `notify.send` jest zapisywane przez rdzeń do outbox i wykonywane po commit transakcji.

## Lifecycle

1. Rdzeń spawnuje proces perimeter.
2. `initialize` + handshake wersji kontraktu.
3. `tools/list`.
4. `tools/call` (z timeoutem).
5. `shutdown` przy zakończeniu.
