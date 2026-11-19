# NEXT.md — instrukcja dla Craft Anything

**Cel bieżący:** TUI terminalowe dla agenta ArndtAi, wzorowane na Grok Build.

## Co już działa (nie ruszaj)

- Pętla, bramki, MCP, sandbox, SQLite recovery (Faza B zamknięta).
- Eventy JSONL w `.agent/sessions/<id>.jsonl` — kontrakt w `docs/contracts/events.md`.
- CLI: `cargo run -- demo`, `cargo run -- -w WS "zadanie"`, `--resume`.

## Zadanie

Zbuduj **tryb TUI** (`ardntai tui` lub flaga `--tui`):

1. **REA** (obowiązkowe przed kodem):  
   Zrewersuj https://github.com/xai-org/grok-build  
   Skup się na `crates/codegen/xai-grok-pager` (TUI: scrollback, prompt, rendering tool calls).  
   Zapisz notatkę w `docs/re/grok-build-tui.md` (co wzięto, co odrzucono).

2. **Minimalny TUI** (ratatui lub podobne, bez nowych ciężkich deps):
   - Scrollback: wyświetla eventy z bieżącej sesji (task, thought, tool_call, tool_result, denied, summary).
   - Prompt na dole: użytkownik wpisuje zadanie, Enter wysyła do pętli.
   - Status bar: kroki / budżet / denied / status (DONE / FAILED_*).
   - Tool calls widoczne w czasie rzeczywistym (streaming z eventów).

3. **Integracja**:
   - TUI korzysta z istniejącej pętli (`agent.rs`) i store.
   - Nie duplikuj logiki bramek ani sandboxu.
   - Sesja zapisywana jak dotychczas (JSONL + SQLite).

4. **Akceptacja**:
   - `cargo build` przechodzi.
   - `cargo run -- tui` otwiera ekran, można wpisać zadanie, widać tool call + wynik.
   - Demo CLI nadal działa.

## Czego nie robić

- Nie buduj web UI w tym kroku.
- Nie zmieniaj kontraktu eventów.
- Nie dodawaj Python core.
- Nie implementuj pełnego Grok Build (tylko wzorzec TUI).

## Po tym kroku

Następny prompt: web UI (Faza C) albo FTS pamięci — według `STAN-2026-11-13.md`.
