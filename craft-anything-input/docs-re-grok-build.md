# RE — Grok Build TUI (do wykonania)

Źródło: https://github.com/xai-org/grok-build

## Co wziąć

- Full-screen TUI (ratatui-style): scrollback + prompt + status.
- Streaming tool calls do scrollback (nie czekaj na koniec).
- Mouse + keyboard (Tab przełącza focus).
- Headless mode (`-p`) jako osobny tryb — opcjonalnie później.

## Czego nie brać

- Cały harness / tool layer Grok Build.
- ACP / editor embedding.
- Skills/plugins/hooks system (mamy własne).

## Mapowanie na ArndtAi

| Grok Build | ArndtAi |
|------------|---------|
| Scrollback | Eventy JSONL (`events.md`) |
| Prompt | Wejście do pętli (`agent.rs`) |
| Tool streaming | `tool_call` / `tool_result` |
| Status | `summary` + liczniki z store |

Notatka po REA: `docs/re/grok-build-tui.md`.
