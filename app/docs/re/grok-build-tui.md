# REA: pager TUI z Grok Build (`xai-org/grok-build`)

Data: 2026-11-13. Commit klonu: `2bdd1d6`, ścieżka lokalna:
`reference/grok-build`. Cel: kopia układu i zachowania pagera w `ardntai tui`,
nie kopiowanie kodu — tylko odtworzone reguły.

## Jak czytane

Klon lokalny (bez sieci), pliki czytane wprost z drzewa:

| plik oryginału | co z niego ustalono |
|---|---|
| `crates/codegen/xai-grok-pager/src/views/agent.rs` | `AgentViewLayout::compute` — kolejność i wysokości wierszy, `SHORT_TERMINAL_ROWS = 16`, `SCROLLBACK_MIN_ROWS = 5`, `AUTO_COMPACT_MAX_ROWS = 20`, reguła gaszenia `cta/follow_ups` na niskim terminalu, `prompt_focus_hint`, budowa podpowiedzi (`build_hints`) |
| `.../xai-grok-pager/src/views/status_bar.rs` | widget górnego paska: `left/center/right`, kolor `theme.gray` na `bg_base`, brak ramek, strażnik kolizji (`center_x > left + 2`) |
| `.../xai-grok-pager/src/views/agent_status.rs` | pasek statusu jako lista itemów, wyrównanie do prawej, separator `│` między itemami |
| `.../xai-grok-pager/src/views/status_line/{mod.rs,segments.rs}` | dolny wiersz statusu: segmenty `cwd/model/ctx`, separator `" │ "`, przygaszony ton, `inner_width` z paddingiem |
| `.../xai-grok-pager/src/views/turn_status.rs` | wiersz aktywności: spinner (`braille_spinner_frames`, `SPINNER_DIVISOR = 4`), `Run <tytuł>`, timer, `[stop]` |
| `.../xai-grok-pager/src/views/shortcuts_bar.rs` | format `klawisz: opis`, separator `"  │  "`, klawisz bold, opis muted, przycinanie do szerokości |
| `.../xai-grok-pager/src/views/prompt_widget/mod.rs` | ramka promptu: górna `╭─╮` z podpisem przy prawej, boczne `│`, dolna `╰─╯` z `info_block` (model · flagi), prefiks `❯`, kursor, placeholder „Build anything" rysowany po treści, kolory `prompt_border` / `prompt_border_active` |
| `.../xai-grok-pager/src/scrollback/layout.rs` | `HorizontalLayout`: akcent 1 kolumna + `block_pad_left` + treść + `block_pad_right` |
| `.../xai-grok-pager/src/scrollback/blocks/user.rs` | zadanie: prefiks `❯ `, brak akcentu, zawijanie |
| `.../xai-grok-pager/src/scrollback/blocks/thinking.rs` | myśl: nagłówek `Thought`/`Thinking…`, rail `┃` przygaszony |
| `.../xai-grok-pager/src/scrollback/blocks/tool/{read,edit,other,mod}.rs` | nagłówki narzędzi: bold czasownik + ścieżka w kolorze `path`, brak akcentu przy zwiniętym bloku, `verb_group_kind` (Reading/Read, Editing/Edited, Sent, Ran), podgląd wyniku i błąd pod nagłówkiem |
| `.../xai-grok-pager/src/scrollback/blocks/{system,session_event}.rs` | komunikaty systemowe muted bez akcentu, znaczniki końca tury (`Worked for …`, `Turn failed: …`) |
| `.../xai-grok-pager/src/app/agent_view/render.rs` | pasek statusu prawy (itemy), `fill_background`, kolejność rysowania |
| `.../xai-grok-pager-render/src/theme/groknight.rs` | paleta `groknight`: tła `#0a0a0a/#0c0c0c/#141414/#242424`, tekst `#e1e1e1/#c8c8c8`, szarości `#6c6c6c/#585858/#787878`, akcenty TokyoNight `#7aa2f7`, `#7dcfff`, `#9ece6a`, `#bb9af7`, `#ff9e64`, `#f7768e`, `#e0af68`, `#1abc9c`; `prompt_border #323237`, `prompt_border_active #505058` |
| `.../xai-grok-pager-render/src/glyphs.rs` | `❯ `, `❙`, `┃`, `◇`, `✓`, `✗`, 8 klatek braille, `  │  `; warianty ASCII tylko dla ConHost |
| `.../xai-grok-pager-render/src/appearance/config.rs` | `LayoutConfig::default()`: `outer_vpad 1`, `outer_hpad 2/2`, `block_pad 2/2`; compact: hpad 1, vpad 0 |

## Co skopiowane (plik oryginału → element w ArndtAi)

| oryginał | u nas (`app/src/tui.rs`) |
|---|---|
| `AgentViewLayout::compute` (`views/agent.rs:178-300`) | `layout()` — ta sama kolejność: pasek (1), odstęp, scrollback (`Min(5)`), odstęp + aktywność, odstęp promptu, prompt (3), status (1), podpowiedzi (1); `SHORT_TERMINAL_ROWS`, `SCROLLBACK_MIN_ROWS`, `AUTO_COMPACT_MAX_ROWS` bez zmian wartości |
| `LayoutConfig::default()` | stałe `OUTER_VPAD/OUTER_HPAD/BLOCK_PAD_LEFT/BLOCK_PAD_RIGHT`, przełączane przy `compact` |
| `StatusBar` (`views/status_bar.rs`) | `App::draw_top` — lewo `ArndtAi · <mózg>`, prawo `workspace · sesja …`, ten sam kolor i tło, brak ramek |
| `AgentStatusBar` (`views/agent_status.rs`) | separator `" │ "` i wyrównanie w `draw_status` |
| `status_line` (`views/status_line`) | `draw_status`: `kroki N/max │ narzedzia N │ odmowy N │ <stan>`, segmenty przygaszone, stan w kolorze (`DONE` zielony, `FAILED_*` czerwony, `pracuje` morski) |
| `turn_status` | `draw_turn`: spinner braille, `Run <ścieżka>`, czas w sekundach; ta sama kadencja (`SPINNER_DIVISOR = 4`) |
| `shortcuts_bar` | `draw_hints`: `Enter: send`, `Ctrl+L: clear`, `PgUp/PgDn: scroll`, `Ctrl+C: quit`, separator `"  │  "`, klawisz bold |
| `prompt_widget` (`show_borders`, `info_block`) | `draw_prompt`: `╭─ podpis ─╮`, `│ ❯ tekst │`, `╰─ model · pamiec · sesja ─╯`, kursor blokowy migający, placeholder po treści |
| `HorizontalLayout` | `draw_scrollback`: kolumna akcentu (1), `block_pad_left = 2`, treść, prawy pad 2, pasek przewijania w ostatniej kolumnie |
| `UserPromptBlock` (`scrollback/blocks/user.rs`) | `Kind::Task` → pas `bg_light` (#242424) pod całym wpisem, prefiks `❯ ` w `accent_user` (#c8c8c8), tekst `text_primary` (#e1e1e1), bez akcentu; `GREEN_DARK` zostaje przy diffie |
| `ThinkingBlock` | `Kind::Thought` → `Thought  krok N` + treść, akcent `❙`, rail `┃` |
| `ReadToolCallBlock` / `EditToolCallBlock` / `OtherToolCallBlock` | `Kind::Call/Ok/Fail/Cached/Skipped`: bold czasownik + ścieżka `path` (pomarańcz), `+N B`, podgląd muted, błąd czerwony, wynik w miejscu wywołania |
| `PermissionView` (odmowa) | `Kind::Denied` → `✗ Odmowa  <narzędzie> — <powód>`, akcent czerwony |
| `SessionEventBlock` | `Kind::Summary` → znacznik stanu muted (`DONE · kroki …`) bez akcentu |
| `SystemMessageBlock` | `Kind::Info/Error` → `◇ …` / `✗ …` |
| `groknight` | stałe `BG/BG_LIGHT/BG_TERMINAL/FG/FG_DARK/GRAY/GRAY_DIM/GRAY_BRIGHT/MAGENTA/ORANGE/CYAN/GREEN/RED/YELLOW/TEAL/PROMPT_BORDER*` |
| `glyphs.rs` | `PROMPT_ARROW`, `ACCENT`, `ACCENT_BAR`, `DIAMOND_HOLLOW`, `BALLOT_X`, `SPINNER[8]`, `SEPARATOR`, `HINT_SEPARATOR` |
| `scrollback/state/layout.rs`, `state/groups.rs` (`gap_after: 1`) | `App::rows` — dokładnie jeden pusty wiersz między wpisami, także po bloku wieloliniowym |
| `views/dashboard/peek.rs` + `views/dashboard/layout.rs` | `split_list_and_peek`, `App::draw_peek`, `peek_preview`, `last_status` — panel po prawej liście: status + wiek, ostatnie linie transkryptu; `PEEK_MIN_BOX_LIVE_TAIL = 8`, `PEEK_MAX_FRAC_NUM/_DEN = 3/8`, `MAX_LIVE_TAIL_ROWS = 28`, panel znika przed `MIN_DASHBOARD_WIDTH = 40` |

Mapowanie eventów (`docs/contracts/events.md`) na bloki pagera — bez zmiany
kontraktu, bez nowych typów:

| event ArndtAi | blok pagera | rysowane jako |
|---|---|---|
| `task` | `UserPromptBlock` | `❯ tekst` |
| `thought` | `ThinkingBlock` | `Thought  krok N` + treść |
| `tool_call` | `ToolCallBlock` (running) | `<Reading/Writing/Sending/Running> <ścieżka>` + spinner w wierszu aktywności |
| `tool_result` | `ToolCallBlock` (done/error) | `<Read/Wrote/Sent/Ran> <ścieżka>  +N B` + podgląd / `✗ KOD` |
| `tool_result_cached` | `ToolCallBlock` (cached) | to samo + `z zapisu` |
| `tool_result_skipped` | `ToolCallBlock` (killed) | to samo + `pominiety (powód)` |
| `denied` | `PermissionView` | `✗ Odmowa …` |
| `summary` | `SessionEventBlock` | `DONE/FAILED_* · kroki …` |
| `store`, `store_error`, `error` | `SystemMessageBlock` | `◇ …` / `✗ …` |

## Czego nie skopiowano (i dlaczego)

- **Cały harness** (ACP, `agent-client-protocol`, subagenty, worktree, MCP w
  pagerze): ArndtAi ma własną pętlę, bramki i perimetr; drugi harness złamałby
  „jedno źródło prawdy o stanie".
- **Model Elm (Action/Effect)**: `event_loop.rs` ma 262 kB, `app_view.rs`
  286 kB — koszt bez zysku przy siedmiu typach eventów.
- **Markdown, syntect, diffy, mermaid, obrazki inline**: kontrakt daje `output`
  jako JSON; kolorowanie i diff to osobny krok.
- **Motywy przełączane w locie, OSC11, kwantyzacja kolorów**: jedna paleta RGB;
  terminal bez truecolor i tak pokaże najbliższy kolor.
- **Zaznaczanie tekstu i schowek pagera, wyszukiwanie w scrollbacku, kolejka
  promptów, `minimal/` (tryb `-p`)**: poza zakresem tej rundy.
- **`[stop]` i `[send to bg]`** w wierszu aktywności: ArndtAi nie ma
  przerywania tury — `Ctrl+C` kończy program, zgodnie z zamówieniem.
- **Pasek zadań/todo/planu, `dock`, `btw`, `goal`**: nie mają odpowiednika
  w eventach ArndtAi; rysowanie ich byłoby zmyśleniem stanu.

## Celowe odstępstwa od oryginału

1. **Górny pasek**: pager w widoku agenta ma tam wyłącznie itemy wyrównane do
   prawej. Zamówienie wymaga nazwy/modelu, workspace i statusu sesji, więc
   lewa strona to `ArndtAi · <mózg>`, prawa `workspace · sesja`; formatowanie
   (kolor, brak ramek, przycinanie) zostało z `StatusBar`.
2. **Czasowniki narzędzi**: pager ma osobne bloki `Read`, `Edit`/`Creating`,
   `Sent`; ArndtAi ma trzy narzędzia, więc `fs.read → Read/Reading`,
   `fs.write → Wrote/Writing`, `notify.send → Sent/Sending`. Wynik zastępuje
   wpis wywołania w tym samym miejscu (pager zmienia blok, nie dopisuje).
3. **Język**: etykiety stanu i podpowiedzi po polsku (`kroki`, `narzedzia`,
   `odmowy`, `pracuje`, `Odmowa`), bo tak mówi reszta programu. Znaki, kolory
   i układ bez zmian.
4. **Prompt**: placeholder „Zadanie dla ArndtAi…" zamiast „Build anything";
   reszta ramki (podpis, info o modelu, kursor, kolory) jak w oryginale.
5. **Mysz**: włączony capture i kółko przewija scrollback; zaznaczanie tekstu
   zostaje po stronie terminala (Shift+drag), bo pagerowe zaznaczanie to
   osobny podsystem.

## Czego nie udało się ustalić

- **Nie uruchomiłem oryginalnego pagera** (wymaga konta i modelu), więc odstępy
  pikselowe i tempo animacji są odtworzone z kodu, nie zmierzone. Wartości
  metryk są jednak wprost z `LayoutConfig::default()` i stałych w
  `views/agent.rs`.
- **Brak zrzutu ekranu oryginału** do porównania 1:1; układ sprawdzony
  odczytem kodu i testem, że panele mieszczą się w ekranie 24–200 kolumn i
  6–60 wierszy.
- **`turn_status` ma w oryginale więcej wariantów** (pytanie do użytkownika,
  watchery, kolejka) — odtworzony tylko wariant „narzędzie w toku" i
  „myślenie", bo tylko te stany istnieją w ArndtAi.

## Runda 6: ekran powitalny i modal „Resume session"

Źródłem prawdy o układzie są teraz **zrzuty realnego renderu** Grok Build
1.0.77 (motyw `groknight`), leżące w repo poza `app/`:
`reference/grok-cli-screens/welcome.txt` (83 kolumny, 19 wierszy treści)
i `reference/grok-cli-screens/resume-modal.txt` (85 kolumn, 53 wiersze).
Pliki nie są kopiowane do `app/` — zrzuty były czytane przy przepisywaniu
geometrii, a te same kolumny są zakodowane w testach `welcome::tests`
(szerokości 83, 120 i 40 kolumn).

### Zrzut referencyjny → nasz ekran

| zrzut referencyjny | nasz ekran (`app/src/welcome.rs`) | różnica |
|---|---|---|
| `  ~/projects/ArndtAi` w kolumnie 2 | `short_path(workspace)` w kolumnie 2, `GRAY` | ścieżka liczona z realnego workspace'u, nie zaszyta |
| logo: 7 wierszy × 14 kolumn, lewa krawędź w kolumnie 35 | `LOGO` 1:1 znakami braille, `logo_x = 2 + (content_w - 14)/2` | 83 → 35, 120 → 54, 40 → 14 (wyśrodkowane w kolumnie treści, jak w zrzucie) |
| menu: etykieta w 17, skrót kończy się w 68 | blok 51 kolumn wyśrodkowany w treści (`menu_x = 2 + (content_w - 51)/2`), etykieta od lewej, skrót do prawej krawędzi bloku | przy 83 wypada dokładnie 17/68, przy 120 blok stoi w 35–86 |
| `Grok 4.7 is here!` / `Select 'Grok 4.7' under /model.` w kolumnie 17 | dwie linie ogłoszeń w `menu_x` z tekstem ArndtAi | treść własna (marketing oryginału), wcięcie bez zmian |
| `   Tip: Try out workflows using /workflows.` w kolumnie 3 | `Tip: Ctrl+R wznawia sesje, Ctrl+W otwiera liste.` w kolumnie 3 | treść i skrót nasze |
| composer: `╭` + 79×`─` + `╮` od kolumny 2, `│ ❯` w środku | `composer_rows()`: szerokość `width-2`, `❯` o kolumnę od ramki | 1:1 |
| dolna krawędź `╰──… Grok 4.7 (medium) · always-approve ─╯` | `╰──… <model> · <tryb> ─╯`; etykieta kończy się w kolumnie `width-4`, po niej `─` i `╯` | model z konfiguracji (`Brain::model_name`), tryb z allowlisty narzędzi (`always-approve`, przy niepełnej `ask`) |
| `Grok Build  1.0.77 [stable]` do prawej krawędzi | `ArndtAi 0.1.0` do prawej krawędzi | wersja z `Cargo.toml` (`env!("CARGO_PKG_VERSION")`), bez kanału `[stable]` |
| modal: pełna szerokość (85), panel zaczyna się pod wierszem ścieżki, 52 wiersze | `resume()`: panel = cała szerokość, `height-1` wierszy pod ścieżką | przy 120×38 panel ma 120×37, a lista 30 wierszy |
| `│  Resume session` (tekst w kolumnie 3) + `[✗]` przy samej ramce | to samo; `[✗]` kończy się w kolumnie `width-2` | 1:1 |
| `/ to search` w kolumnie 2, `<n> sessions` przy prawej | to samo; licznik kończy się trzy kolumny od prawej | w zrzucie po liczniku widać urwane `Grok f` — to resztka poprzedniej klatki, nie element paska; nie kopiujemy |
| `│────…────│` między paskiem narzędzi a listą | ten sam wiersz: kreski między bokami ramki | 1:1 |
| `No matches` w kolumnie 3 | to samo, gdy filtr albo szukanie nic nie zostawia | 1:1 |
| wpisy listy (w zrzucie 0 sesji) | tytuł + przygaszony ` · subtitle`, wiek przy prawej, zaznaczenie paskiem `bg_light` (#242424) | format wpisu z zamówienia i z naszego okna listy; `…` przy ucięciu tytułu |
| stopka `Esc:back  │  Enter:select  │  ctrl+w:worktree  │  ↑/↓:navigate  │  f:filter`, 7 spacji do ramki | te same separatory `  │  ` i te same etykiety | 1:1 przy 85; poniżej 46 kolumn segmenty są ucinane od końca (zostaje `Esc:back  │  Enter:select`) |

### Różnice, których nie da się przenieść

- **Logo** to znak oryginału, przeniesiony znakami ze zrzutu na wyraźne
  zamówienie. Nie jest to znak ArndtAi — jeśli projekt dostanie własny, wymienia
  się jedną stałą `LOGO`.
- **Wersja i kanał**: `Grok Build  1.0.77 [stable]` → `ArndtAi 0.1.0`;
  numer bierzemy z `Cargo.toml`, kanału wydania nie mamy.
- **Nazwa modelu i trybu**: `Grok 4.7 (medium) · always-approve` → realny stan
  (`<model> · <tryb>`); przy braku klucza API jest `offline (mock)`.
- **Ogłoszenia i `Tip:`** są własne; w zrzucie reklamują wydanie Grok 4.7.
- **Pion**: zrzut pokazuje 19 wierszy treści od góry i nie mówi, co robi
  oryginał na wyższym terminalu. U nas treść jest wyrównana do góry, a resztę
  ekranu wypełnia tło — przy 38 wierszach pod wersją zostaje puste miejsce.
  Tego jednego elementu nie da się potwierdzić z jednego zrzutu.
- **`Grok f` w pasku narzędzi** modala to artefakt odświeżenia ekranu, nie
  element układu — świadomie pominięty.

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
| stopka `Shift+Tab:mode │ Ctrl+c:cancel │ Ctrl+.:shortcuts` | `draw_footer`: `Enter:send │ Ctrl+F:szukaj │ Ctrl+L:clear │ Ctrl+C:quit` (runda 17 zamieniła `PgUp/PgDn:scroll` na `Ctrl+F` — przewijanie opisuje `app/docs/TUI.md`), ten sam format i to samo miejsce (wiersz pod composerem) | klawisze są nasze i naprawdę działają; po prawej, przygaszone `kroki N/max · narzędzia N · odmowy N · <stan>` — te liczniki są z pętli, oryginał ich tam nie ma (u niego w tym miejscu nic) |
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
(960×672 px), żeby kadr odpowiadał zrzutowi referencyjnemu. (W rundzie 8
domyślny kadr wrócił do 120×38 = 960×608 — patrz niżej.)

## Runda 8: powitanie i modal — domknięcie wierności

Zamówienie wskazywało cztery odstępstwa ekranu powitalnego (wcięcie `Tip:`,
wcięcie ogłoszeń, rozmiar logo, dwie spacje przed ścieżką) i pełną ramkę modala.
Sprawdzenie układu w buforze przy 83 kolumnach pokazało, że `welcome::welcome`
i `welcome::resume` już wypadają znak po znaku jak `welcome.txt` /
`resume-modal.txt`: ścieżka w kolumnie 2, logo 7×14 od kolumny 35, menu
w kolumnach 17–68, ogłoszenia w 17, `Tip:` w 3, composer od 2 do 82; w modalu
`┌ ┐ └ ┘`, `│` po obu stronach, `├───┤` pod tytułem, separator pod paskiem
narzędzi, `[✗]` kończący się w kolumnie `width-2` i stopka od kolumny 2.
Odchylenia widoczne w starych zrzutach pochodziły z rysowania powitania
w `tui.rs` sprzed rundy 7 — od rundy 7 oba ekrany rysuje wyłącznie
`welcome.rs`, więc zrzuty idą za referencją.

Co naprawdę odbiegało, to rasteryzator PNG (a nie układ znaków):

| objaw w PNG | przyczyna | poprawka |
|---|---|---|
| logo niższe i „grubsze” niż w terminalu | braille rysowany jako 2×2 px w połowie komórki 8×16 | [`braille_glyph`](../src/shot.rs): 2 kolumny × 4 rzędy punktów 3×3 px w pełnej komórce |
| boki panelu kreskowane, narożniki jak `+` | `│` miał 8 px w komórce 16 px, a `┌ ┐ └ ┘ ├ ┤` spadały do `fallback` = `+` | [`box_glyph`](../src/shot.rs): pełne 16 px i prawdziwe narożniki/trójniki |
| kadr 960×672 zamiast 960×608 | runda 7 ustawiła 120×42 pod zrzut rozmowy | domyślnie 120×38 = 960×608; `--screenshot* plik.png 83x38` daje kadr referencyjny |

Logo zostaje 14 kolumn × 7 wierszy, bo taki jest znak w `welcome.txt`
i w oryginalnym `assets/logo/logo07.txt` (7 niepustych linii, szerokość 14).
Opis „~40 kolumn, kilkanaście wierszy” nie znajduje potwierdzenia w żadnym
z tych dwóch źródeł — powiększenie go kosztowałoby zgodność linii ze zrzutem.
Po poprawce rasteryzatora ten sam znak zajmuje w PNG pełne 16 px na wiersz
(wcześniej 8 px), więc w obrazku jest dwa razy wyższy i cieńszy.

Testy: `welcome::tests::logo_is_the_reference_shape` (kształt 1:1 z
`welcome.txt`), `shot::tests::box_and_braille_glyphs_fill_the_whole_cell`
(pełna komórka), istniejące testy 83/120/40 kolumn i 85 kolumn modala bez zmian.
Zrzuty `app/shots/tui-welcome.png` i `tui-resume.png` są teraz 960×608.
