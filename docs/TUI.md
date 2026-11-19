# Tryb TUI

`ardntai tui` (albo `ardntai --tui`) otwiera pełnoekranowy ekran w terminalu.
Bez zadania startuje na **liście sesji** (odpowiednik dashboardu Grok Build);
z zadaniem w argumencie — od razu na ekranie sesji z wpisanym zadaniem.

Wzorzec i mapowanie plików oryginału: `docs/re/grok-build-tui.md`.
Port czytany z lokalnego klonu `reference/grok-build` (commit `2bdd1d6`).

## Ekran startowy — lista sesji

```
~/p/ArndtAi/app                       ◆ 4 gotowe │ ◆ 1 błąd
+ Nowa sesja                              Filtr: wszystkie  /
▾ Gotowe 4 ───────────────────────────────────────────────────
▏ ◆ write raport.txt :: alfa; read                       1m
▏   Wrote raport.txt
▾ Błąd 1 ─────────────────────────────────────────────────────
  ◆ write a.txt :: x; read a.txt            failed        1m
    FAILED_BUDGET  budzet krokow wyczerpany (1)

  ╭────────────────────────────────────────────────────────────╮
  │❯ Nowe zadanie… (Enter startuje, n nowa sesja)              │
  ╰────────────────────────────────────────────────────────────╯
↑↓: wybor  │  Enter: otworz  │  n: nowa sesja  │  /: filtr  │  Esc: wyjscie
```

Dane: wyłącznie `.agent/sessions/*.jsonl` i `.agent/resume` — żaden wiersz nie
jest zmyślony. Pusty katalog pokazuje „Brak sesji w .agent/sessions”.

Wiersz sesji (3 linie, jak `dashboard/row.rs` + `row_title.rs` + `row_activity.rs`):

| element | skąd |
|---|---|
| znacznik `▏` + ikona stanu (`◆` / `◇` / spinner) | `RowState` |
| tytuł | pierwsze `task` (30 znaków) albo `Nowa sesja #<id>` |
| ` · subtitle` przygaszony | gałąź gita, gdy jest |
| badge `worktree`, `needs-input`, `failed` | tylko gdy prawdziwe |
| wiek po prawej (`1m`, `2h`) | ostatni event |
| druga linia | ostatnie narzędzie / ostatnia myśl / `Pending: …`; bez aktywności — liczniki |
| trzecia linia | oddech (`ROW_HEIGHT = 3`) |

Nagłówek: gałąź, badge `worktree`, ścieżka skrócona (`~/…`), po prawej liczniki
niezerowych stanów. Drugi wiersz: `+ Nowa sesja` i `Filtr: <stan>  /`.
Wszystko z paddingiem 2 kolumn (`HEADER_OUTER_HPAD`, `LIST_OUTER_HPAD`).

Grupy w kolejności: czeka na Ciebie → w toku → bezczynna → gotowe → błąd.

## Ekran sesji

```
ArndtAI · offline (mock)      sesja 17945827 · krok 6/12      screenshot-ws
  ...scrollback z akcentem ┃ po lewej...
  ◆ Read notatka.txt   +18 B
    ArndtAI TUI działa
  ┃ Run fs.read  (wiersz tury, tylko gdy leci tura)
  ╭─────────────────────────────────────── zadanie ─╮
  │❯ Zadanie dla ArndtAI…                            │
  ╰──────────────────────────────────────────────────╯
kroki 6/12 · narzedzia 4 · odmowy 1 · DONE     offline (mock) · pamiec lite
Enter: send  │  Ctrl+L: clear  │  PgUp/PgDn: scroll  │  Ctrl+C: quit
```

Bloki scrollbacku (znaki z `scrollback/blocks/*`): `❯` zadanie, `◆ Thought`,
`◆ Czasownik ścieżka` (w toku — akcent `┃`, puls), `✓`/`✗` wynik pod nagłówkiem
w tym samym miejscu, `+N B` rozmiar, `·` magazyn. Dla `fs.write` dochodzi blok
`diff`: numer linii i treść na tle `diff_insert_bg` (nowy plik = same wstawienia).

Geometria pionowa z `AgentViewLayout::compute`:

| element | wysokość |
|---|---|
| górny pasek | 1 |
| scrollback | `Min(5)`, resztę zabiera |
| wiersz tury | 1 + 1 wiersz odstępu (tylko gdy tura leci) |
| odstęp promptu | 1 (0 na krótkim terminalu) |
| composer | `2 + wiersze tekstu`, min 3, maks. połowa ekranu |
| dolny status | 1 |
| podpowiedzi | 1 |

## Zwijanie przy małym terminalu

- `≤ 20` wierszy (`AUTO_COMPACT_MAX_ROWS`) — tryb kompaktowy: bez pionowego
  marginesu, padding boczny 1 kolumna.
- `≤ 16` wierszy (`SHORT_TERMINAL_ROWS`) — najpierw znika wiersz tury, potem
  odstęp promptu; scrollback, composer i pasek statusu zostają.
- `< 40` kolumn (`MIN_DASHBOARD_WIDTH`) — wiersz listy traci badge i subtitle,
  wiersz akcji zwija się do `+ nowa` i `/ <filtr>`, środek górnego paska znika
  przed nachodzeniem na boki. Żaden tekst nie wychodzi za ekran.

## Klawisze

Lista sesji:

- `↑` / `↓` — wybór wiersza (gdy pole nowego zadania jest puste).
- `Enter` — wejście w zaznaczoną sesję; z wpisanym tekstem — nowa sesja z tym zadaniem.
- `n` — nowa sesja.
- `/` — filtr: wszystkie → błędy → w toku.
- `Esc` — wyjście z programu.
- Pisanie w polu na dole (litery, `Backspace`, strzałki, `Home`/`End`).
  Skróty `n` i `/` działają tylko przy pustym polu, żeby nie zjadały liter.

Sesja:

- `Enter` — wyślij zadanie do pętli (`agent::run`).
- `PgUp` / `PgDn` — przewijanie historii; `PgDn` do końca wraca do „na żywo”.
- `Up` / `Down` — historia wpisanych zadań.
- `Esc` — powrót do listy sesji.
- `Ctrl+L` — czyści scrollback (transkrypt zostaje na dysku).
- `Ctrl+U` — czyści wpisane zadanie.
- `Ctrl+C` / `Ctrl+D` — wyjście.
- Kółko myszy przewija scrollback.

## Jak to jest podłączone

TUI nie ma własnej pętli: woła `agent::run`, a `context::Session::event`
oprócz zapisu linii do `.agent/sessions/<id>.jsonl` wysyła tę samą linię do
sinku TUI (`context::set_event_sink`). Kontrakt `docs/contracts/events.md`
i format JSONL zostają bez zmian — to ten sam strumień, tylko z drugim
konsumentem. Lista sesji tylko czyta te pliki.

Dlatego działają bez zmian: bramki (allowlista, scope guard, dryf), budżet
kroków, SQLite + recovery, outbox `notify.send` i `--provider mock`.

Narzędzia perimeteru: `fs.read`, `fs.write`, `fs.append`, `notify.send`.
Detektor dryfu liczy tylko mutacje (`fs.write`, `fs.append`, `notify.send`);
`fs.read` nie wchodzi do okna i nie może wywołać odmowy — odmowę dla odczytu
daje wyłącznie scope guard.

## Zrzuty ekranu

- `ardntai --screenshot plik.png` — ekran sesji (demo, 120 klatek) → 960×608.
- `ardntai --screenshot-list plik.png` — ekran listy na prawdziwych sesjach
  z `.agent/sessions` bieżącego workspace'u → 960×608.

Oba rysują do bufora, zamieniają znaki na piksele i zapisują PNG. Nie dotykają
terminala, nie otwierają okna i nie zabierają fokusu; kończą się po zapisie.
Workspace zrzutu sesji jest jednorazowy i usuwany.

## Bez terminala

Gdy stdin/stdout nie jest terminalem (rura, CI), `ardntai tui` nie rysuje
ekranu: z zadaniem w argumencie robi zwykły przebieg liniowy, bez zadania
wypisuje instrukcję i kończy się kodem 0.

## Ograniczenia tej rundy

- Brak procentu kontekstu w wierszu listy: transkrypt nie zapisuje użycia
  kontekstu, a zmyślona liczba byłaby gorsza niż brak.
- Brak peeku/podglądu sesji, worktree pickera, modalów i kolejki promptów —
  to panele oryginału bez odpowiednika w ArndtAi.
- Markdown i mermaid w scrollbacku nie są renderowane; kontrakt daje tekst.
- Jedna paleta (`groknight`), bez przełączania motywów.
- Brak zaznaczania i wyszukiwania tekstu w scrollbacku.
