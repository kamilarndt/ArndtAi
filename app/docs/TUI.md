# Tryb TUI — układ pagera Grok Build

`ardntai tui` (albo `ardntai --tui`) otwiera pełnoekranowy ekran w terminalu.
Układ, metryki, paleta i znaki są skopiowane z pagera `xai-grok-pager`
(commit `2bdd1d6`, klon w `reference/grok-build`). Mapa plik oryginału →
element u nas: `app/docs/re/grok-build-tui.md`.


## Ekrany i wejście

`ardntai tui` bez zadania otwiera **ekran powitalny**; z zadaniem w argumencie
— od razu sesję. Ekrany:

```
Welcome  ──ctrl+r──▶  Resume session (modal)  ──Enter──▶  Session
   │                        │                              │
   ├──ctrl+w──▶ Sessions ◀──┘ (ctrl+w)                     │
   ├──ctrl+n──▶ Session (nowa, pusta) ◀────────────────────┘ (Esc w sesji)
   └──ctrl+q──▶ koniec
```

Ekran listy sesji (`Sessions`) zostaje bez zmian (nagłówek, grupy, peek, filtr),
ale wejściem do sesji jest teraz modal — lista jest pod `ctrl+w`.

### Ekran powitalny (bez sesji, po `ctrl+n`)

```
  ~/projects/ArndtAi                                        ← ścieżka (przygaszona)
                                   ⠀⠀⠀⠀⠀⠀⣀⣀⡀⠀⠀⠀⢀⠄        ← logo, wyśrodkowane
                 Resume session                               ctrl+r
                 New worktree                                 ctrl+w
                 Changelog
                 Quit                                         ctrl+q
                 ArndtAi TUI jest gotowe.
                 Wybierz 'Resume session' albo wpisz zadanie.
   Tip: Ctrl+R wznawia sesje, Ctrl+W otwiera liste.
  ╭───────────────────────────────────────────────────────────────────────────────╮
  │ ❯                                                                             │
  ╰────────────────────────────────────── offline (mock) · always-approve ─╯
                                                        ArndtAi 0.1.0
```

| element | treść |
|---|---|
| ścieżka | skrócona do `~/…`, kolumna 2 |
| logo | 7 wierszy braille, wyśrodkowane w kolumnie treści (przy 83 kolumnach: 35) |
| menu | etykieta od lewej, skrót wyrównany do prawej krawędzi bloku 51 kolumn (17–68 przy 83) |
| etykieta composera | `<model> · <tryb>`: model z konfiguracji providera, tryb z allowlisty narzędzi (`always-approve`, przy niepełnej `ask`) |
| linia wersji | `ArndtAi <wersja z Cargo.toml>`, do prawej krawędzi |

### Modal „Resume session" (`ctrl+r`)

```
  ~/projects/ArndtAi
┌───────────────────────────────────────────────────────────────────────────────────┐
│  Resume session                                                                [✗]│
├───────────────────────────────────────────────────────────────────────────────────┤
│ / to search                                                    5 sessions         │
│───────────────────────────────────────────────────────────────────────────────────│
│  write raport.txt :: alfa; read                                              1h   │
│  write ../poza.txt :: x; write                                               1h   │
│  No matches                                                                       │
│ Esc:back  │  Enter:select  │  ctrl+w:worktree  │  ↑/↓:navigate  │  f:filter       │
└───────────────────────────────────────────────────────────────────────────────────┘
```

Lista to prawdziwe wpisy z `.agent/sessions/*.jsonl` (tytuł, przygaszony
` · subtitle`, wiek przy prawej); zaznaczenie to pasek tła `bg_light`
(#242424). Gdy filtr albo szukanie nic nie zostawia, w miejscu listy stoi
`No matches`. Licznik pokazuje liczbę sesji po filtrach.

| klawisz | efekt |
|---|---|
| `ctrl+r` (na powitaniu) | otwórz modal na świeżo odczytanych transkryptach |
| `/` | szukanie w tytule: kolejne znaki zawężają listę |
| `f` | filtr: wszystkie → błędy → w toku |
| `↑` / `↓` | zaznaczenie na liście |
| `Enter` | otwórz zaznaczoną sesję (transkrypt wraca do scrollbacku) |
| `Backspace` | usuń znak szukania |
| `Esc` | w szukaniu: wyczyść zapytanie; poza szukaniem: wróć na powitanie |
| `ctrl+w` | przejdź do listy sesji (dashboard) |

### Klawisze ekranu powitalnego

| klawisz | efekt |
|---|---|
| `ctrl+r` | modal „Resume session" |
| `ctrl+w` | lista sesji |
| `ctrl+n` | nowa, pusta sesja |
| `Enter` | wyślij wpisane zadanie (nowa sesja) albo otwórz pustą |
| `ctrl+q`, `Esc` | wyjście |

## Ekran od góry

```
ArndtAi · openai / gpt-4o        sesja 1a2b3c4d · kroki 3/12        moj-projekt
                                                                            ← 1 wiersz, bez ramki
  ❯ write a.txt :: alfa; read a.txt                                         ← zadanie (bez akcentu)
                                                                            ← odstęp
  ◆ Thought  krok 1                                                         ← akcent ❙ magenta
  ┃   krok 1 z planu offline: fs.write                                      ← rail ┃ przygaszony
  ❙ Reading a.txt                                                           ← wywołanie w toku
  ❙ Read a.txt  +4 B                                                        ← wynik w tym samym miejscu
  ❙   alfa                                                                  ← podgląd wyniku
  ❙ ✗ Odmowa  fs.write — sciezka zawiera '..'                               ← odmowa, akcent czerwony
     DONE · kroki 3 · narzedzia 2 · odmowy 1 · dryf 0                       ← znacznik stanu, bez akcentu
                                                                            ← pusty wiersz
  ╭──────────────────────────────────────────────────────────── zadanie ─╮
  │❯ Zadanie dla ArndtAi…                                                  │
  ╰──────────────────────── openai / gpt-4o · pamiec lite ────────────────╯
  kroki 3/12 │ narzedzia 2 │ odmowy 1 │ DONE
  Enter: send  │  Ctrl+L: clear  │  PgUp/PgDn: scroll  │  Ctrl+C: quit
```

| obszar | treść | skąd w oryginale |
|---|---|---|
| górny pasek | lewo: nazwa + mózg (`offline (mock)`, `anthropic / ...`, `openai / ...`); środek: `sesja <8> · kroki N/max` (przygaszony, wyrównany do środka szerokości, znika zanim dotknie boków); prawo: workspace | `views/status_bar.rs` (trzy strefy, kolor `theme.gray` / `theme.gray_dim`, tło `bg_base`) |
| scrollback | bloki `task`, `thought`, `tool_call`, `tool_result`, `denied`, `summary` | `scrollback/blocks/*` + `docs/contracts/events.md` |
| wiersz aktywności | spinner + `Run <narzędzie>` + czas, tylko gdy leci zadanie | `views/turn_status.rs` |
| prompt | ramka `╭─╮ │ ╰─╯`, podpis `zadanie`, `❯`, kursor blokowy, info o modelu | `views/prompt_widget/mod.rs` (`show_borders`, `info_block`) |
| dolny status | `kroki N/max`, `narzedzia N`, `skille N`, `odmowy N`, stan (`DONE` / `FAILED_*` / `pracuje`) | `views/status_line` (segmenty, separator ` │ `) |
| podpowiedzi | `klawisz: opis` rozdzielone `  │  ` | `views/shortcuts_bar.rs` |

Metryki (1:1 z `LayoutConfig::default()`): pad zewnętrzny 2 kolumny i 1 wiersz,
wiersz wpisu = akcent 1 kolumna + 2 kolumny paddingu + treść + 2 kolumny,
scrollback dostaje resztę ekranu (w oryginale `Min(5)`), auto-compact od
20 wierszy (pad 1/0), krótki terminal ≤ 16 wierszy traci odstęp nad promptem.

## Bloki i znaki

| event | rysowane jako |
|---|---|
| `task` | `❯ tekst` (jasny, bez akcentu) |
| `thought` | `Thought  krok N` + treść pod spodem (akcent `❙`, rail `┃`) |
| `tool_call` | `<Reading/Writing/Sending/Running> <ścieżka>` + akcent `❙` |
| `tool_result` | `<Read/Wrote/Sent/Ran> <ścieżka>  +N B` + podgląd, w miejscu wywołania |
| `tool_result` (błąd) | to samo + `✗ KOD` i treść błędu na czerwono |
| `tool_result_cached` | `✓`-owy wariant z dopiskiem `z zapisu` |
| `tool_result_skipped` | dopisek `pominiety (powod)` w kolorze ostrzeżenia |
| `denied` | `✗ Odmowa  <narzędzie> — <powód>` na czerwono |
| `summary` | `DONE/FAILED_* · kroki N · narzedzia N · skille N · odmowy N · dryf N` |
| `store`, `error` | linia systemowa `◇ ...` / `✗ ...` |

Spinner: 8 klatek braille, dzielnik 4 ticki (20 fps → 5 fps), jak
`SPINNER_DIVISOR` w oryginale.

## Klawisze

| klawisz | efekt |
|---|---|
| `Enter` | wyślij zadanie do pętli (`agent::run`) |
| `Esc`, `Ctrl+C`, `Ctrl+D` | wyjście |
| `PgUp` / `PgDn` | przewijanie scrollbacku o 5 wierszy; `PgDn` do końca wraca „na żywo" |
| kółko myszy | przewijanie o 3 wiersze (jak w pagerze) |
| `Up` / `Down` | historia wpisanych zadań |
| `Left` / `Right` / `Home` / `End` | ruch kursora w promptcie |
| `Backspace` / `Delete` | edycja |
| `Ctrl+U` | wyczyść prompt |
| `Ctrl+L` | wyczyść scrollback (transkrypt zostaje na dysku) |
| `Ctrl+F` | szukanie w scrollbacku (modal: `/ fraza`, licznik trafień; `n`/`N` następne, `Esc` powrót) |

Po statusie `FAILED*` (`FAILED_ERROR`, `FAILED_BUDGET`, `FAILED_STALLED`)
ostatnio wysłane zadanie wraca do composera z kursorem na końcu — nie trzeba
go pisać od nowa. Błąd zostaje w scrollbacku (event `error`) i w pasku stanu.
Tekst wpisany już po wysłaniu nie jest nadpisywany, a po `DONE` composer
zostaje pusty jak dotychczas.

Zmiana rozmiaru okna jest liczona od nowa przy każdej klatce: układ mieści się
w ekranie od 6 wierszy w górę, a poniżej 16 wierszy traci opcjonalne odstępy
zamiast ściskać prompt albo scrollback.

## Licznik kontekstu

Prawy górny róg pokazuje `<zużyte> / <limit.context>`. Limit pochodzi z
`GET {base_url}/models` (pole `limit.context`), pobranego raz na start sesji
z cache `.agent/models.json` (TTL 1 h); brak sieci/klucza → cache, brak cache
→ `?`. Zużycie: realne tokeny z `usage` odpowiedzi, a gdy provider go nie
przysyła — szacunek z znaków transkryptu (≈4 znaki na token) z jawnym `~`,
np. `~1.5K / 1.0M`. Szczegóły: `app/docs/re/tui-model-and-commands.md`.

## Komendy `/`

Wpisanie `/` na początku composera otwiera listę podpowiedzi pod composerem;
prefiks filtruje, `Tab`/`Enter` uzupełnia, `Esc` chowa. Rejestr jest w
`src/commands.rs` (`/help`, `/model`, `/settings`, `/theme`, `/clear`,
`/resume`, `/skills`, `/skill`, `/copy`, `/export`, `/quit`). `/help` wypisuje tabelę w scrollbacku, `/model` otwiera
modal wyboru modelu (wybór zapisywany w `.agent/config`, `ARNDTAI_MODEL` ma
pierwszeństwo), `/settings` modal ustawień, `/theme jasny|groknight` zmienia
motyw od razu, a nieznana komenda daje komunikat zamiast błędu.

## Skille (runda 16)

`/skills` otwiera modal listy: nazwa, opis, rozmiar treści i znacznik
`✓ w kontekście`; `↑/↓` wybór, `Enter` podgląd treści w scrollbacku, `Esc`
zamknięcie. `/skill <nazwa>` wczytuje pełną treść do kontekstu następnej tury
i potwierdza to w scrollbacku; nieznana nazwa daje komunikat. Pasek statusu
pokazuje `skille N` (ile odkryto). Pełny opis formatu, kolejności katalogów
i kosztu kontekstu: `app/docs/re/skills.md`.

## Szukanie, kopiowanie, eksport (runda 17)

`Ctrl+F` otwiera modal `Search scrollback` w stylu „Select model": pasek
`/ fraza`, licznik `1/3`, lista trafień jako numer wiersza + tekst. `Esc`
zamyka i wraca na miejsce otwarcia, `Enter` zamyka modal i zostawia
podświetlone trafienie w scrollbacku (`n`/`N`, także `Ctrl+N`/`Ctrl+P`).
`/copy` kopiuje ostatnią odpowiedź
asystenta przez OSC 52, `/copy all` — całą rozmowę; `/export [plik]` zapisuje
transkrypt jako Markdown do workspace (te same bramki ścieżki co `fs.write`).
Skróty, format eksportu i ograniczenia OSC 52: `app/docs/re/search-copy-export.md`.

## Ustawienia (runda 15)

Sześć kluczy w `.agent/config` — `theme`, `approval`, `memory`, `timestamps`,
`mouse`, `max_tokens` — czytanych przy starcie i zmienianych w modalu
`/settings` (`↑/↓` wybór, `Enter`/`←→` zmiana, `Esc` zamknięcie). Zła wartość
w pliku daje wartość domyślną i ostrzeżenie w scrollbacku, nigdy crash.
Precedencja: `ARNDTAI_MODEL`/`ARNDTAI_TIMEOUT` → `--memory` → plik → domyślna.

- `approval = ask` pyta w composerze przed każdym `fs.write`/`fs.append`/
  `notify.send`: `Zezwolić na fs.write nota.txt? [t/n]`. `n` nie tworzy pliku,
  transkrypt dostaje `denied` z `USER_DENIED`, licznik odmów rośnie, sesja
  żyje dalej.
- `theme = jasny`: tło `#f5f5f5`, tekst `#1f1f1f`, akcent `#7c3aed`, zieleń
  `#2f7d32`; układ bez zmian (`src/theme.rs`).
- `max_tokens` to sufit odpowiedzi, nigdy więcej niż `limit.output` modelu.
- `timestamps = off` chowa godzinę przy wpisie użytkownika, `mouse = off`
  wyłącza przechwytywanie kółka i kliknięć.

Szczegóły i mapa testów: `app/docs/re/settings.md`.


## Prawdziwy provider

`--provider openai` wysyła dodatkowo nagłówek `x-opencode-session` (bramka
OC-Go odrzuca żądanie bez niego: `MissingSessionID`). Wartość:

1. `ARNDTAI_SESSION`, jeśli ustawiona,
2. inaczej losowy identyfikator raz na proces (pid + czas + licznik).

Nagłówek idzie do pliku konfiguracji `curl` (0600) tak samo jak klucz API —
nigdy do `argv`. Prompt pokazuje pierwsze 8 znaków sesji na dole ramki.

Limit czasu jednego żądania to `ARNDTAI_TIMEOUT` w sekundach (domyślnie 300),
przekazywany do `curl --max-time`. Wartość niepoprawna (pusta, nie-liczba, 0)
kończy program kodem 2 z komunikatem wskazującym zmienną — bez cichego
podstawiania domyślnej.

## Jak to jest podłączone

TUI nie ma własnej pętli: woła `agent::run`, a `context::Session::event`
oprócz zapisu linii do `.agent/sessions/<id>.jsonl` wysyła tę samą linię do
sinku TUI (`context::set_event_sink`). Kontrakt `docs/contracts/events.md`
i format JSONL zostają bez zmian — to ten sam strumień, tylko z drugim
konsumentem. Dlatego bez zmian działają bramki (allowlista, scope guard, dryf),
budżet kroków, SQLite + recovery, outbox `notify.send` i `--provider mock`.

Narzędzia perimeteru: `fs.read`, `fs.write`, `fs.append`, `notify.send`.
Detektor dryfu liczy tylko mutacje (`fs.write`, `fs.append`, `notify.send`);
`fs.read` nie wchodzi do okna i nie może wywołać odmowy — odmowę dla odczytu
daje wyłącznie scope guard.

## Zrzut ekranu

`ardntai --screenshot plik.png` rysuje ekran sesji do bufora (120 klatek),
zamienia znaki na piksele (znak 8×16, font 8×8 plus własne wzory ramek,
akcentów i braille) i zapisuje PNG 960×608 (120×38 znaków). Nie dotyka
terminala, nie zabiera fokusu, nie otwiera okna — kończy się po zapisaniu pliku.
Workspace zrzutu jest jednorazowy i usuwany po zapisie. Przykład:
`app/shots/tui-clone.png`.

Pozostałe ekrany mają własne flagi (wszystkie 960×608, ten sam rasteryzator):

| flaga | ekran | przykład |
|---|---|---|
| `--screenshot` | sesja (scrollback + composer) | `app/shots/tui-clone.png` |
| `--screenshot-welcome` | ekran powitalny | `app/shots/tui-welcome.png` |
| `--screenshot-resume` | modal „Resume session" na prawdziwych transkryptach | `app/shots/tui-resume.png` |
| `--screenshot-list` | lista sesji (dashboard) | `app/shots/tui-list.png` |

Do porównania ze zrzutem referencyjnym o innej szerokości każda z flag
przyjmuje opcjonalny rozmiar `WxH` w znakach:

```sh
ardntai --screenshot-welcome /tmp/welcome-83.png 83x38
ardntai --screenshot-resume  /tmp/resume-85.png  85x53
```

Rasteryzator rysuje braille i ramki w pełnej komórce 8×16: punkt braille ma
3×3 px (2 kolumny × 4 rzędy), a `│` przechodzi przez całe 16 px, więc boki
panelu nie są kreskowane, a narożniki `┌ ┐ └ ┘` i trójniki `├ ┤` domykają
ramkę tak jak znaki w terminalu.

## Bez terminala

Gdy stdin/stdout nie jest terminalem (rura, CI), `ardntai tui` nie rysuje
ekranu: z zadaniem w argumencie robi zwykły przebieg liniowy, bez zadania
wypisuje instrukcję i kończy się kodem 0.
