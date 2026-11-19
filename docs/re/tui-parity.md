# Parytet TUI z Grok Build — plan i stan

**Data:** 2026-11-14. Wzorzec: `reference/grok-build` (commit 2bdd1d6, Apache-2.0),
TUI = `crates/codegen/xai-grok-pager` (649 plików, 540 865 linii).

Zasada: **przenosimy zachowanie i wygląd, nie kod.** Ich drzewo nie buduje się bez
`protoc`/`dotslash`, a pager ciągnie 10+ wewnętrznych crate'ów — fork byłby
droższy niż port. Każda pozycja niżej ma odpowiednik w naszym kodzie.

## Stan po rundach 14–18 (2026-11-14)

| obszar Groka | u nas | stan |
|---|---|---|
| ekran powitalny, logo, menu | `welcome.rs` | ✅ port ze zrzutu 1.0.77 |
| ekran rozmowy (pas, `◆ Thought`, diff, status) | `tui.rs` | ✅ port ze zrzutu README |
| lista sesji + peek | `sessions.rs` + `tui.rs` | ✅ z `.agent/sessions/*.jsonl` |
| modal „Resume session" (`/ to search`) | `tui.rs` | ✅ ramka + wyszukiwanie |
| klawisze (Enter, Ctrl+F/L/U, PgUp/PgDn, mysz, historia) | `tui.rs` | ✅ |
| scrollback bloków | `tui.rs` | ✅ wynik w miejscu wywołania |
| paleta `groknight` + motyw jasny | `tui.rs` | ✅ dwie palety |
| okno kontekstu z metadanych modelu | `models.rs` | ✅ `limit.context`, `?` gdy brak |
| model picker (`/model`) | `models.rs` + `tui.rs` | ✅ wybór trwały, env ma pierwszeństwo |
| rejestr komend + `/help` + podpowiedzi `/` | `tui.rs` | ✅ 8 komend |
| ustawienia (`/settings`) | `settings.rs` | ✅ 6 pozycji, zapis w `.agent/config` |
| tryb zgód `ask` (pytanie przed zapisem) | `settings.rs` + `agent.rs` | ✅ `USER_DENIED` |
| skille (`SKILL.md` → kontekst, `skill.load`) | `skills.rs` | ✅ 3 katalogi, limit 8 KB |
| wyszukiwanie w historii (`Ctrl+F`) | `tui.rs` | ✅ licznik trafień, skoki |
| kopiowanie (`/copy`, OSC 52) | `tui.rs` | ✅ przez terminal, bez xclip |
| eksport (`/export`) | `tui.rs` | ✅ markdown z krokami |
| markdown w odpowiedziach | `tui.rs` | ✅ nagłówki, listy, kod, tabela, cytat |
| tips / changelog / tutorial | — | ⚠️ statyczne linie, bez treści Groka |
| voice, share, plugin, workflow, dashboard wielu agentów, subagenty | — | ⛔ wymaga ich runtime'u |

Liczby po rundzie 18: **231 testów jednostkowych + 15 integracyjnych**, demo 39/39.
Grok ma 73 komendy; u nas jest 8, bo tylko dla tylu mamy realne dane i efekt.

## Kolejność rund

1. **R14 — model i metadane:** `GET /models` (OpenAI-compatible) → okno kontekstu,
   limit wyjścia, koszt; `/model` jako picker; rejestr komend + `/help`
   + podpowiedzi po wpisaniu `/`. Koniec ze sztywnym `500K`.
2. **R15 — ustawienia:** `/settings` (motyw, tryb zgód `ask|always-approve`,
   poziom pamięci `lite|pro|ultra`, znaczniki czasu, mysz), zapis w `.agent/config`.
3. **R16 — skille:** loader `skills/<nazwa>/SKILL.md` do kontekstu, `/skills`
   z listą i podglądem, licznik użytych skilli w statusie.
4. **R17 — historia i tekst:** wyszukiwanie w scrollbacku, zaznaczanie, kopiowanie,
   `/export`.
5. **R18 — markdown:** nagłówki, listy, kod z podświetlaniem w scrollbacku.

Poza zakresem (wymaga runtime'u Groka): dashboard wielu agentów, subagenty,
voice, share, plugins, workflows, ACP.

## Źródła prawdy do porównania

- zrzuty: `reference/grok-cli-screens/` (`welcome.txt`, `resume-modal.txt`,
  `readme-tui.png`)
- kod: `crates/codegen/xai-grok-pager/src/{views,scrollback,slash,settings}`,
  paleta `xai-grok-pager-render/src/theme/groknight.rs`
- lista komend: `slash/commands/` (73 pliki) — nazwy do przeniesienia wybiórczo,
  tylko te, dla których mamy realne dane.
