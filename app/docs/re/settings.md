# Runda 15: `/settings`, tryb zgód `ask`, motyw `jasny`

Data: 2026-11-14. Zakres: prawdziwe ustawienia zamiast placeholdera, pytanie
o zgodę przed efektem ubocznym, druga paleta i limit odpowiedzi. Kontrakty
(`docs/contracts/`), schemat bazy, bramki, dryf i allowlista bez zmian; żadnego
web UI i żadnych pozostałych komend Groka.

## 1. Klucze configu

Plik: `.agent/config`, format `klucz = wartość` (ten sam, w którym picker
`/model` zapisuje `model`). Zapis podmienia tylko wskazany wiersz
(`models::config_set`), więc komentarze i pozostałe klucze zostają.

| klucz | wartości | domyślna | skutek |
|---|---|---|---|
| `theme` | `groknight`, `jasny` | `groknight` | paleta całego TUI |
| `approval` | `always-approve`, `ask` | `always-approve` | pytanie przed `fs.write`/`fs.append`/`notify.send` |
| `memory` | `lite`, `pro`, `ultra` | `lite` | poziom pamięci, jak `--memory` |
| `timestamps` | `on`, `off` | `on` | godzina `HH:MM` przy wpisie użytkownika |
| `mouse` | `on`, `off` | `on` | `EnableMouseCapture` (kółko i kliknięcia) |
| `max_tokens` | `1024`, `2048`, `4096`, `8192` | `4096` | sufit `max_tokens` żądania |

Odczyt: `settings::Settings::load(workspace)` w `App::new` (raz na start
procesu) — czyta tylko te sześć kluczy i nie dotyka niczego poza plikiem.
Zapis: każde `←`/`→`/`Enter` w modalu woła `Settings::save`, czyli sześć
`config_set` po kolei; `/theme jasny` zapisuje sam `theme`.

## 2. Precedencja

```
zmienne środowiskowe  →  argv (--memory)  →  .agent/config  →  wartość domyślna
```

- `ARNDTAI_MODEL` nadal wygrywa z `model` z pliku (bez zmian z rundy 14).
- `ARNDTAI_TIMEOUT` nadal jest jedynym źródłem limitu czasu żądania; zła
  wartość to błąd klienta (exit 2), nie ciche 300 s.
- `--memory ultra` w argv wygrywa z `memory` z pliku. Bez flagi obowiązuje
  plik, a bez wpisu — `lite`.
- Oba klucze środowiskowe są w modalu tylko **pokazane**: wiersz z etykietą
  zmiennej, wartością i dopiskiem `z env`. Zmiana strzałkami jest odrzucana
  komunikatem „ta wartosc pochodzi ze srodowiska", bo środowisko ma
  pierwszeństwo i edycja pliku nic by nie dała.

## 3. Zła wartość w pliku

Nieznana wartość **nie** zatrzymuje programu i nie jest naprawiana po cichu:

1. klucz dostaje wartość domyślną (`theme = neon` → `groknight`),
2. do scrollbacku trafia ostrzeżenie z nazwą klucza i wpisaną wartością:
   `config: theme = 'neon' nieznane - zostaje groknight`,
3. ostrzeżenia wracają po otwarciu nowej sesji (`Ctrl+N`, transkrypt z listy) —
   `startup_warnings` w `App`,
4. pierwszy zapis z modala nadpisuje zły wiersz poprawną wartością.

Brak klucza to po prostu wartość domyślna — bez ostrzeżenia. Brak pliku też.

## 4. Modal `/settings`

`/settings` bez argumentów otwiera modal; `/settings` z ekranu powitalnego
najpierw otwiera świeżą sesję (composer jest tylko tam). Ramka i pasek tytułu
są te same co w „Select model" (`welcome::settings`, `welcome::picker`):
ścieżka w nagłówku, `┌─ Settings ─…─ [✗]`, `├─…─┤`, lista, stopka klawiszy,
notatka po ostatniej zmianie pod ramką.

- `↑`/`↓` — wybór wiersza (zaznaczenie `●`, pas `bg_light`),
- `Enter` / `→` — następna wartość, `←` — poprzednia (cyklicznie),
- `Esc` — zamknięcie modala, bez zapisu poza już wykonanymi zmianami,
- każda zmiana działa od razu: motyw przemalowuje ekran, mysz włącza/wyłącza
  przechwytywanie, `approval` zmienia podpis composera (`model · tryb`),
  `memory` idzie do następnej tury pętli, `max_tokens` do następnego żądania.

`/theme jasny` i `/theme groknight` ustawiają motyw bez otwierania modala.
`/theme` bez argumentu otwiera modal, literówka daje podpowiedź z dozwolonymi
nazwami. Rejestr komend ma teraz siedem pozycji (`/theme` jest jedyną z
`needs_arg`), a `/help` oznacza ją gwiazdką.

## 5. Tryb `ask`

Domyślnie `always-approve`: nic się nie zmienia, żadnego pytania i żadnych
kanałów między wątkami. Przy `approval = ask`:

1. Agent mija bramki jak dotychczas (allowlista → scope guard → dryf).
2. Tuż przed efektem — a **przed** zapisem kroku `proposed` — woła
   `Approver::ask(tool, arg)`: `fs.write`/`fs.append` podają ścieżkę,
   `notify.send` kanał. `fs.read` nie pyta (nie ma efektu ubocznego).
3. TUI dostaje prośbę w `poll()` i rysuje ją w composerze:
   `Zezwolić na fs.write nota.txt? [t/n]`, w ramce `fs.write · ask`.
4. `t` (albo `Enter`) — zgoda, narzędzie idzie normalnie (`tool_call` →
   `tool_result`). `n` (albo `Esc`) — odmowa:
   - efekt **nie** powstaje, krok nie trafia do SQLite jako `proposed`
     (inaczej po crashu wyglądałby jak przerwany przebieg),
   - transkrypt dostaje `{"kind":"denied","data":{"step":…,"tool":…,
     "reason":"USER_DENIED","drift":false}}` — te same pola co każda inna
     odmowa, więc kontrakt `events.md` zostaje bez zmian,
   - licznik `odmowy` w stopce rośnie, a model dostaje w historii
     `ok=false -> USER_DENIED: …` i próbuje dalej,
   - odmowa **nie** zwiększa serii `FAILED_STALLED`: użytkownik ma prawo
     odmówić i nie zabija to sesji.
5. Gdy TUI padnie przed odpowiedzią, kanał się zamyka i bramka traktuje to
   jak odmowę — bezpieczna strona.

Wątek pętli stoi na bramce do odpowiedzi; reszta UI (rysowanie, spinner)
działa dalej, bo to osobny wątek.

## 6. Motyw `jasny`

`src/theme.rs`. Układ i wszystkie reguły rysowania zostają w `tui.rs`; motyw
to wyłącznie paleta. Ekran rysowany jest paletą `groknight`, a na koniec
`App::draw` robi jeden przebieg `theme::recolor` po komórkach bufora.

| element | `groknight` | `jasny` |
|---|---|---|
| tło | `#141414` | `#f5f5f5` |
| tekst | `#e1e1e1` | `#1f1f1f` |
| akcent (myśl, romb) | `#bb9af7` | `#7c3aed` |
| zieleń (DONE) | `#9ece6a` | `#2f7d32` |
| pas zaznaczenia | `#242424` | `#e8e8ec` |
| ramka composera | `#323237` | `#cdcdd4` |

Bez przezroczystości i gradientów — kolory kwantyzują się do 16 kolorów
terminala bez utraty kontrastu. Kolory spoza palety (np. z wypełnień bufora)
są odwracane jasnością: ciemne jaśnieją, jasne ciemnieją, więc nic nie zostaje
białe na jasnym tle.

## 7. Limit odpowiedzi

`max_tokens = 1024|2048|4096|8192` zastępuje sztywne `4096` z rundy 14.
Wartość idzie do `Config::max_tokens` jako
`min(ustawienie, limit.output modelu)` (`Settings::token_budget`):

- model z `limit.output = 131072` i ustawienie `4096` → `4096`,
- ten sam model i ustawienie `8192` → `8192`,
- model z `limit.output = 1000` → `1000` (nigdy więcej niż model dopuszcza),
- brak metadanych modelu → samo ustawienie.

Ścieżka: `/settings` → `.agent/config` → `Settings` → `agent::Config` →
`Brain::from_env_full(max_tokens)` → `RemoteBrain.max_tokens` → ciało żądania
(`max_tokens` u Anthropica i OpenAI). Osobny wiersz w configu czyta też CLI
(`arndtai run`), żeby zachowanie było jedno.

Test na lokalnym atrapie: `tests/cli.rs::config_max_tokens_goes_into_the_request`
— config `max_tokens = 1024`, atrapa HTTP zapisuje ciało żądania, asercja
`body["max_tokens"] == 1024`; bez wpisu w configu zostaje `4096`.

## 8. Testy tej rundy

| co | test |
|---|---|
| config: 6 kluczy w obie strony, zły klucz → domyślna + ostrzeżenie | `settings::tests::*` |
| ceny: oba kształty providera, brak → `—` | `models::tests::prices_read_both_provider_shapes` |
| `ask`: odmowa blokuje efekt, `USER_DENIED`, licznik | `agent::tests::ask_mode_user_denial_blocks_the_effect` |
| `ask`: zgoda wykonuje efekt | `agent::tests::ask_mode_user_approval_runs_the_tool` |
| `ask`: `fs.read` nie pyta | `agent::tests::ask_mode_does_not_ask_about_read_only_tools` |
| `ask`: pytanie w composerze, `n` odblokowuje wątek | `tui::tests::ask_question_waits_in_the_composer_for_t_or_n` |
| modal: 6 wierszy, zapis, `Esc` | `tui::tests::settings_modal_shows_six_keys_and_writes_the_file` |
| zły plik → ostrzeżenie w scrollbacku | `tui::tests::settings_from_a_broken_file_warn_in_the_scrollback` |
| motyw: tło i akcent, powrót `groknight` | `tui::tests::light_theme_repaints_background_and_accents_and_groknight_comes_back` |
| `/theme jasny` od razu | `tui::tests::theme_command_switches_the_palette_immediately` |
| znaczniki czasu | `tui::tests::timestamps_follow_the_setting` |
| ramka i wiersze modala | `welcome::tests::settings_modal_*` |
| `max_tokens` w żądaniu (atrapa) | `tests/cli.rs::config_max_tokens_goes_into_the_request` |

Zrzuty (960×608, `--screenshot-settings`, `--screenshot-ask`,
`--screenshot-light`) leżą w `checks/round-15/`.
