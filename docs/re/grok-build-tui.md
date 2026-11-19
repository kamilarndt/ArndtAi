# REA: pager TUI z Grok Build (`xai-org/grok-build`)

Data: 2026-11-13. Cel: **port ekranów** pagera do `ardntai tui`, nie rekonstrukcja
z pamięci. Kod czytany z lokalnego klonu, nie z pamięci ani z opisów.

## Źródło

- Klon: `reference/grok-build` (tylko do czytania; nie zmieniamy go).
- Commit: `2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8` (2026-09-29).
- Czytane moduły:
  - `crates/codegen/xai-grok-pager/src/views/agent.rs` — `AgentViewLayout::compute`,
    stałe `SHORT_TERMINAL_ROWS`, `SCROLLBACK_MIN_ROWS`, `AUTO_COMPACT_MAX_ROWS`,
    `turn_status_height`, `prompt_gap`, `Constraint::Min(SCROLLBACK_MIN_ROWS)`.
  - `crates/codegen/xai-grok-pager/src/scrollback/layout.rs` — `HorizontalLayout::new`
    (akcent 1 kolumna, `block_pad_left/right`).
  - `crates/codegen/xai-grok-pager/src/scrollback/blocks/` — `user.rs`,
    `thinking.rs`, `tool/` (`mod.rs`, `edit.rs`), `system.rs`, `agent.rs`,
    `markdown_content.rs`, `mermaid_content.rs`, `bg_task.rs`, `subagent.rs`,
    `workflow.rs`, `btw.rs`, `context_info.rs`, `quote_bar.rs`, `cancel_cause.rs`.
  - `crates/codegen/xai-grok-pager/src/views/dashboard/` — `layout.rs` (`MIN_DASHBOARD_WIDTH`,
    `LIST_FLOOR_ROWS`, `PEEK_*`, `MAX_LIVE_TAIL_ROWS`, `HEADER_OUTER_HPAD`,
    `LIST_OUTER_HPAD`, `DISPATCH_OUTER_HPAD`), `row.rs`, `row_title.rs`,
    `row_activity.rs`, `chrome.rs` (`render_header`, `render_actions_row`),
    `render.rs` (`render_rows_with_grouping`, `render_narrow_rows_with_grouping`),
    `state.rs` (`RowState`, `Filter`).
  - `crates/codegen/xai-grok-pager-render/src/theme/groknight.rs` — paleta.
  - `crates/codegen/xai-grok-pager/src/settings/defs.rs` — domyślny motyw `groknight`.
  - `crates/codegen/xai-grok-pager/src/views/context_bar.rs` — `SEPARATOR = "│"`.

## Geometria przeniesiona (wartości bez zmian)

| stała oryginału | wartość | u nas |
|---|---|---|
| `SHORT_TERMINAL_ROWS` | 16 | `SHORT_TERMINAL_ROWS` (`tui.rs`) |
| `SCROLLBACK_MIN_ROWS` | 5 | `SCROLLBACK_MIN_ROWS` |
| `AUTO_COMPACT_MAX_ROWS` | 20 | `AUTO_COMPACT_MAX_ROWS` |
| `MIN_DASHBOARD_WIDTH` | 40 | `MIN_DASHBOARD_WIDTH` |
| `LIST_FLOOR_ROWS` | 12 | `LIST_FLOOR_ROWS` — dolna granica listy przed oddaniem miejsca peekowi |
| `HEADER_OUTER_HPAD` / `LIST_OUTER_HPAD` / `DISPATCH_OUTER_HPAD` | 2 | 2 kolumny w `list_layout` |
| `HorizontalLayout`: akcent / `block_pad_*` | 1 / 2 / 2 | `ACCENT`, `BLOCK_PAD_LEFT`, `BLOCK_PAD_RIGHT` |
| `vpad` widoku agenta | 1 (0 przy `compact`) | `OUTER_VPAD` / `COMPACT_HPAD` |
| pasek statusu | 1 wiersz, zawsze | `Screen::top`, 1 wiersz |
| `prompt_gap` | 1, 0 na krótkim terminalu | `prompt_gap` w `layout()` |
| `turn_status_height` | 1 + 1 wiersz odstępu | `Screen::turn` |
| wysokość composera | `vpad_top + tekst + info_block`, min 3, maks. połowa ekranu | `want = 2 + prompt_rows`, `max_prompt` |
| `prompt` przy `rows <= 16` | odstęp i wiersz tury znikają pierwsze | pętla zwijania w `layout()` |

Kolejność pionowa z `AgentViewLayout::compute` (bez paneli, których ArndtAi nie ma):
pasek statusu → odstęp → scrollback (`Min(5)`) → [odstęp + wiersz tury] →
`prompt_gap` → composer → pasek statusu dolny → podpowiedzi.

## Mapowanie plik → u nas / pominięte

| plik oryginału | u nas | uwaga |
|---|---|---|
| `views/agent.rs` (`AgentViewLayout::compute`) | `app/src/tui.rs::layout`, `draw_turn` | te same stałe i kolejność; panele todo/queue/dock/btw/banner pominięte — ArndtAi ich nie ma |
| `views/status_bar.rs` | `draw_top`, `draw_status`, `draw_hints` | trzy strefy, środek znika przed nachodzeniem |
| `views/context_bar.rs` | `SEPARATOR` w górnym pasku i statusie | `│` w odstępach |
| `scrollback/layout.rs` (`HorizontalLayout`) | `Row.accent` + `BLOCK_PAD_*` | akcent 1 kolumna, treść wcięta o 2 |
| `scrollback/blocks/user.rs` | `entry_rows` → `Kind::Task` | pas `bg_light` (#242424) pod całym wpisem, prefiks `❯ ` w `accent_user` (#c8c8c8), tekst `text_primary` (#e1e1e1); `GREEN_DARK` zostaje przy diffie |
| `scrollback/state/layout.rs`, `state/groups.rs` (`gap_after`) | `rows()` w `tui.rs` | dokładnie **1** pusty wiersz między wpisami (także po bloku wieloliniowym); wewnętrzne odstępy bloku (nagłówek ↔ treść) bez zmian |
| `views/dashboard/peek.rs` + `layout.rs` (`PEEK_*`, `MAX_LIVE_TAIL_ROWS`) | `split_list_and_peek`, `draw_peek`, `peek_preview` w `tui.rs` | panel po prawej stronie listy: status + wiek, ostatnie linie transkryptu, ramka `selection_border`; stałe `PEEK_MIN_BOX_LIVE_TAIL = 8`, `PEEK_MAX_FRAC_NUM/_DEN = 3/8`, `MAX_LIVE_TAIL_ROWS = 28`; panel znika, zanim lista straci `MIN_DASHBOARD_WIDTH = 40` kolumn |
| `scrollback/blocks/thinking.rs` | `Kind::Thought` | `◆ Thought`, pusty wiersz, treść pod akcentem `┃` w `gray_dim` |
| `scrollback/blocks/tool/mod.rs` | `Kind::Call` / `Kind::Ok` / `Kind::Fail`, `tool_display`, `past_verb`, `present_verb` | `◆ Czasownik ścieżka`; wynik `✓`/`✗`, rozmiar `+N B`, wynik pod nagłówkiem w miejscu |
| `scrollback/blocks/tool/edit.rs` | `diff_rows` | blok `diff` dla `fs.write`: numer linii + treść na tle `diff_insert_bg` (nowy plik = same wstawienia) |
| `scrollback/blocks/system.rs` | `Kind::Info`, wpisy `store` | `· magazyn: …` |
| `scrollback/blocks/agent.rs` (markdown) | pominięte | kontrakt `events.md` daje czysty tekst/JSON; renderowania markdownu nie ma w tej rundzie |
| `blocks/markdown_content.rs`, `blocks/mermaid_content.rs` | pominięte | brak markdownu i mermaid w danych |
| `blocks/bg_task.rs`, `subagent.rs`, `workflow.rs`, `btw.rs`, `context_info.rs`, `quote_bar.rs`, `cancel_cause.rs` | pominięte | ArndtAi nie ma subagentów, zadań w tle, workflow, `/btw`, metadanych kontekstu |
| `views/dashboard/layout.rs` | `list_layout`, stałe `MIN_DASHBOARD_WIDTH`/`HEADER_OUTER_HPAD`/`LIST_OUTER_HPAD`/`DISPATCH_OUTER_HPAD` | `PEEK_MIN_BOX_*`, `PEEK_MAX_FRAC_*`, `MAX_LIVE_TAIL_ROWS`, `LIST_FLOOR_ROWS` — używane w `split_list_and_peek` |
| `views/dashboard/row.rs` (`DashboardRow`, `RowBadge`) | `app/src/sessions.rs` (`SessionRow`, `Badge`) | tytuł = pierwsze zadanie (30 znaków), subtitle = gałąź, badge: `worktree`, `needs-input`, `failed`; `context_pct` pominięte — brak danych |
| `dashboard/row_title.rs` | `draw_session_row` | ` · subtitle` przygaszony, badge'y przy prawym końcu tytułu, `< 40` kolumn → wariant `render_narrow` (bez subtitle i badge'ów) |
| `dashboard/row_activity.rs` | `sessions::activity` | ostatnie narzędzie (czas teraźniejszy/przeszły), `Pending:` żółte, ostatnia myśl |
| `dashboard/state.rs` (`RowState`, `Filter`) | `sessions::State`, `sessions::Filter` | 5 stanów z priorytetem grupy, filtr wszystkie/błędy/w toku |
| `dashboard/chrome.rs` | `draw_list_header`, `draw_list_actions` | padding 2, liczniki stanów po prawej, filtr po prawej |
| `dashboard/render.rs` | `draw_list_rows`, `draw_group_header` | grupa `▾ Etykieta N ───`, wiersz 3 linie, przewijanie okna |
| `theme/groknight.rs` | stałe na górze `tui.rs` | użyte wpisy 1:1; nieużywane: `BLUE`, `BLUE0`, `BLUE1`, `GREEN1`, `RED1`, `BG_DARK`, `BG_STORM_DARK`, `FG_GUTTER`, `DARK3` (markdown/linki/motyw przełączany) |
| `settings/defs.rs` (domyślny `groknight`) | jedna paleta na stałe | brak przełączania motywów — jak w rundzie 1 |

## Czego nie dało się ustalić

- Nie uruchamiałem pagera Grok Build (wymaga konta i modelu), więc **odstępy
  pikselowe i tempo animacji są odczytane z kodu, nie zmierzone**.
- Brak zrzutu ekranu oryginału do porównania 1:1; układ odtworzony z
  `AgentViewLayout::compute` i rendererów.
- `context_pct` w wierszu listy: oryginał bierze z żywego agenta, ArndtAi nie
  zapisuje użycia kontekstu w transkrypcie — pole pominięte zamiast zmyślonej liczby.
- Dispatch, worktree picker i modale: poza zakresem; peek jest, ale **w wersji
  bocznej** (panel po prawej liście), a nie pod listą jak w oryginale — w pagerze
  peek zastępuje composer w `dispatch`, u nas lista i composer zostają bez zmian.
  Z peeku pominięte: `❯ reply` (podgląd nie wysyła wiadomości), obsługa pytań
  o pozwolenie (`PermissionView`), `@`-picker plików i badge modelu.
- `--screenshot` pokazuje **początek** przebiegu, żeby pas zadania i odstępy były
  w kadrze; interaktywne `tui` otwiera się na końcu scrollbacku, jak dotąd.
- Pomiar `--screenshot` (piksele, nie „na oko”): pas zadania `#242424`, prefiks
  `#c8c8c8`, tekst `#e1e1e1`, `#063806` wyłącznie na wierszu diffu; między
  wpisami dokładnie jeden pusty wiersz tekstu (16 px).

## Runda 7: ekran rozmowy wg oficjalnego zrzutu (`readme-tui.png`)

Źródłem prawdy o układzie ekranu sesji jest teraz oficjalny zrzut TUI z README
`xai-org/grok-build`: `reference/grok-cli-screens/readme-tui.png`
(1728×907 px, okno terminala ≈ **120 kolumn × 42 wiersze**, komórka
≈ 8,4 × 15,5 px). Zrzut leży poza `app/` i nie jest kopiowany — z niego
zmierzono liczby, które są teraz w kodzie i w testach.

### Zmierzone z PNG (i gdzie są w kodzie)

| pomiar ze zrzutu | wartość | u nas |
|---|---|---|
| tło treści | `#141414` (20,20,20) | `BG` |
| pas wpisu użytkownika | `#242424` (36,36,36), pełna szerokość treści | `BG_LIGHT` w `Kind::Task` |
| wysokość pasa | 158–203 px ≈ **3 wiersze** (tekst w środkowym) | 3 wiersze: odstęp, tekst, odstęp |
| wcięcie tekstu w pasie | ≈ 2,9 kolumny od lewej krawędzi pasa | `" ❯ "` = 3 kolumny (`BAND_INDENT`) |
| ramka composera | `#323237` (50,50,55), zaokrąglona | `PROMPT_BORDER` |
| skok wiersza | 15,5 px, jeden pusty wiersz między blokami | 1 pusty wiersz (`gap_after: 1`) |
| nagłówek | wiersz 1, potem pusty wiersz, potem pas | `layout()`: `top` (1), odstęp, scrollback |
| znacznik czasu w pasie | `8:19 AM`, przygaszony, przy prawej | `HH:MM` z `clock()` (czas lokalny, `date +%z`), `GRAY_DIM` |
| romb myśli | `◆` w `#bb9af7` (`accent_thinking`) | `MAGENTA` = `#bb9af7` |
| stopka | jeden wiersz pod composerem, separator `│` | `screen.footer`, separator `" │ "` |
| pasek przewijania | prawa krawędź, gdy treść nie mieści się w oknie | ostatnia kolumna scrollbacku |

### Element ze zrzutu → nasz render

| element ze zrzutu | nasz render (`app/src/tui.rs`) | czego nie mamy / co inaczej |
|---|---|---|
| pasek górny: `~/Documents` po lewej, `9.5K / 500K` po prawej | `draw_top`: `short_path(workspace)` + `kcount(context_chars) / 500K`, oba `GRAY`, nic w środku | licznik to **znaki** transkryptu, nie tokeny (provider nie zwraca `usage`); budżet 500 000 znaków to stała `CONTEXT_BUDGET`, nie okno modelu |
| pas użytkownika `#242424` z `❯ zadanie` i godziną | `Kind::Task` → 3 wiersze `BG_LIGHT`, `" ❯ "` w `ACCENT_USER`, tekst `FG`, godzina `HH:MM` przy prawej | godzinę bierzemy z chwili zdarzenia (czas lokalny); oryginał ma `8:19 AM`, my 24 h |
| `◆ Thought for 2.0s` | `Kind::Thought` → `◆ ` `MAGENTA` + `Thought` `GRAY` + ` for X.Ys` `GRAY_DIM` | czas mierzy `Instant` od wysłania zadania do **pierwszej** myśli tury; kolejne myśli, odtwarzanie transkryptu i czas < 0,1 s dają samo `◆ Thought` (nie zmyślamy liczby) |
| tekst asystenta: akapity, pogrubione nagłówki, listy `•`, reguły `—` | `prose_rows()`: nagłówki `#`–`######` bez hashy i boldem, `**…**` boldem w linii, `-`/`*`/`·` → `· `, `---`/`***`/`___` → `——`, akapity rozdzielone jednym pustym wierszem | to **markdown-lite**: brak tabel, kodu, linków, list numerowanych i zawijania wciętego w listach; tekst idzie pełną szerokością, bez ramek i bez kolumny akcentu |
| znacznik czasu przy nagłówku odpowiedzi (`8:19 AM`) | tylko pas zadania ma godzinę | nasze bloki to zdarzenia pętli, nie wiadomości modelu — nie ma do czego przypiąć godziny odpowiedzi |
| `⠋ Responding… 15s` po lewej, `17s ↓9.45k [stop]` po prawej | `draw_stream`: spinner braille + `Responding… Ns`, po prawej `Ns ↓N.NNk` | `k` = tysiące **znaków** zdarzeń tury, nie tokenów; `[stop]` nie jest rysowane, bo pętla nie ma anulowania w locie (`Ctrl+C` kończy program) |
| composer: zaokrąglona ramka, `❯`, etykieta `<model> · <tryb>` wpisana w dolną krawędź po prawej | `draw_prompt`: `╭─╮` / `│ ❯ … │` / `╰─ <model> · <tryb> ─╯`, kursor blokowy, placeholder po treści | model z `Brain::model_name()` (`offline (mock)` bez klucza), tryb z allowlisty (`always-approve` / `ask`); brak `(high)` przy modelu — nie mamy poziomów rozumowania |
| stopka `Shift+Tab:mode │ Ctrl+c:cancel │ Ctrl+.:shortcuts` | `draw_footer`: `Enter:send │ Ctrl+L:clear │ PgUp/PgDn:scroll │ Ctrl+C:quit`, ten sam format i to samo miejsce (wiersz pod composerem) | klawisze są nasze i naprawdę działają; po prawej, przygaszone `kroki N/max · narzedzia N · odmowy N · <stan>` — te liczniki są z pętli, oryginał ich tam nie ma (u niego w tym miejscu nic) |
| pasek przewijania przy prawej krawędzi | track `BG_TERMINAL` + thumb `BG_LIGHT` w ostatniej kolumnie scrollbacku | rysowany tylko przy przepełnieniu; oryginał ma cieńszy, półznakowy wskaźnik |

### Czego nie mamy (świadomie)

- **Tokeny.** Provider nie zwraca `usage`, więc licznik kontekstu i licznik
  strumienia liczą znaki. Format `9.5K / 500K` i `↓9.45k` zostaje, jednostka
  jest inna — odnotowane w tabeli wyżej.
- **Pełny markdown.** Nagłówki, pogrubienie, listy i reguły tak; tabele, bloki
  kodu, linki, obrazy i mermaid nie (kontrakt `events.md` daje czysty tekst).
- **Osobny event odpowiedzi asystenta.** W `events.md` nie ma `assistant_message`
  — tekst modelu to pole `thought` (i `summary.reason` na końcu), więc rysujemy
  go pod nagłówkiem `◆ Thought` jako akapity pełną szerokością. Kontraktu nie
  zmieniamy; dodanie typu byłoby nową wersją `events.md`.
- **`[stop]`.** Pętla `agent::run` nie ma anulowania w trakcie tury; rysowanie
  przycisku, który nic nie robi, byłoby zmyśleniem stanu. `Ctrl+C` kończy
  program razem z turą.
- **Znaczniki czasu per odpowiedź** i **wskaźnik „przewiń do dołu” (`▼`)** —
  nie mają odpowiednika w naszych blokach.
- **Model w pasku górnym.** Zgodnie z zamówieniem pasek ma tylko ścieżkę
  i licznik; model i tryb są w etykiecie composera.

### Co zostało z poprzednich rund

Ekran powitalny, modal „Resume session” i lista sesji są bez zmian (runda 6).
Klawisze (`Ctrl+R`, `Ctrl+W`, `Ctrl+L`, `PgUp/PgDn`, `Enter`, `Esc`, `Ctrl+C`),
zapis sesji (JSONL + SQLite), bramki i sandbox — nietknięte; zmienił się wyłącznie
sposób rysowania ekranu sesji. Zrzut `--screenshot` jest teraz 120×42
(960×672 px), żeby kadr odpowiadał zrzutowi referencyjnemu.
