# ArndtAi — stan i następne kroki (2026-11-13)

**Stan:** M0 + Faza B zamknięte.  
**Następny cel:** TUI terminalowe (wzorzec Grok Build).

## Pliki dla Craft Anything

- `STAN-2026-11-13.md` — co jest, czego nie ma.
- `NEXT.md` — dokładna instrukcja na TUI.
- `docs-re-grok-build.md` — co wziąć z REA.

## Prompt do wklejenia (pierwszy)

```
Przeczytaj NEXT.md i STAN-2026-11-13.md.
Zrób tylko TUI terminalowe według NEXT.md.
Najpierw REA https://github.com/xai-org/grok-build (pager TUI), notatka w docs/re/.
Potem minimalny TUI na ratatui: scrollback eventów + prompt + status.
Nie ruszaj web UI. Nie zmieniaj kontraktów.
cargo build + cargo run -- tui musi działać.
```
